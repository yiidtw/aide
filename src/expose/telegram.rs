use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::path::Path;
use tracing::{error, info, warn};

use crate::agents::agentfile::AgentfileSpec;
use crate::agents::instance::InstanceManager;

const TELEGRAM_API: &str = "https://api.telegram.org";

/// Run a Telegram bot that forwards messages to an agent instance.
/// This blocks forever (long polling loop).
pub async fn run_telegram_bot(
    data_dir: &str,
    instance: &str,
    token: &str,
) -> Result<()> {
    let mgr = InstanceManager::new(data_dir);

    // Verify instance exists
    let _manifest = mgr
        .get(instance)?
        .ok_or_else(|| anyhow::anyhow!("No such instance: {}", instance))?;

    // Get bot info
    let client = reqwest::Client::new();
    let me = tg_request(&client, token, "getMe", &json!({})).await?;
    let bot_username = me["result"]["username"]
        .as_str()
        .unwrap_or("unknown");
    info!(
        bot = %bot_username,
        instance = %instance,
        "telegram bot started"
    );
    println!("Telegram bot @{} → {}", bot_username, instance);
    println!("Send a message to @{} to talk to your agent.", bot_username);

    // Long polling loop
    let mut offset: i64 = 0;
    loop {
        let params = json!({
            "offset": offset,
            "timeout": 30,
            "allowed_updates": ["message"],
        });

        let resp = match tg_request(&client, token, "getUpdates", &params).await {
            Ok(r) => r,
            Err(e) => {
                warn!(error = %e, "telegram poll error, retrying in 5s");
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                continue;
            }
        };

        let updates = resp["result"].as_array().cloned().unwrap_or_default();
        for update in updates {
            let update_id = update["update_id"].as_i64().unwrap_or(0);
            offset = update_id + 1;

            // Extract message text
            let message = &update["message"];
            let text = message["text"].as_str().unwrap_or("");
            let chat_id = message["chat"]["id"].as_i64().unwrap_or(0);
            let from = message["from"]["first_name"]
                .as_str()
                .unwrap_or("unknown");

            if text.is_empty() || chat_id == 0 {
                continue;
            }

            // Skip /start command
            if text == "/start" {
                let welcome = format!(
                    "Hi! I'm connected to agent `{}`. Send me a message and I'll forward it.",
                    instance
                );
                let _ = tg_send(&client, token, chat_id, &welcome).await;
                continue;
            }

            info!(from = %from, text = %text, "incoming message");
            mgr.append_log(instance, &format!("telegram: {} said: {}", from, text))?;

            // Execute skill via semantic mode (pass whole message as skill input)
            let reply = exec_for_telegram(&mgr, instance, text);

            // Send reply
            let reply_text = match reply {
                Ok(output) => {
                    if output.trim().is_empty() {
                        "(no output)".to_string()
                    } else {
                        output
                    }
                }
                Err(e) => format!("Error: {}", e),
            };

            mgr.append_log(instance, &format!("telegram: replied to {}", from))?;

            if let Err(e) = tg_send(&client, token, chat_id, &reply_text).await {
                error!(error = %e, "failed to send telegram reply");
            }
        }
    }
}

/// Spawn telegram bot as a background task (for daemon integration).
pub fn spawn_telegram_bot(data_dir: String, instance: String, token: String) {
    tokio::spawn(async move {
        if let Err(e) = run_telegram_bot(&data_dir, &instance, &token).await {
            error!(
                instance = %instance,
                error = %e,
                "telegram bot exited"
            );
        }
    });
}

/// Execute a skill for telegram — runs the first matching skill or falls back to raw text.
fn exec_for_telegram(mgr: &InstanceManager, instance: &str, text: &str) -> Result<String> {
    let inst_dir = mgr.base_dir().join(instance);

    // Try to parse as "skill args" first
    let parts: Vec<&str> = text.splitn(2, ' ').collect();
    let potential_skill = parts[0].to_lowercase();
    let args = if parts.len() > 1 { parts[1] } else { "" };

    // Check if it matches a known skill
    let skill_script = inst_dir
        .join("skills")
        .join(format!("{}.sh", potential_skill));
    if skill_script.exists() {
        let env = load_scoped_env(&inst_dir, Some(&potential_skill))?;
        let (exit_code, stdout, stderr) =
            exec_skill_script(&skill_script, args, &inst_dir, &env)?;
        if exit_code == 0 {
            return Ok(stdout);
        } else {
            return Ok(format!("{}\n{}", stdout, stderr).trim().to_string());
        }
    }

    // No matching skill — try all skills and return a help message
    if let Ok(spec) = AgentfileSpec::load(&inst_dir) {
        let skill_list: Vec<String> = spec
            .skills
            .iter()
            .map(|(name, def)| {
                let desc = def
                    .description
                    .as_deref()
                    .map(|d| format!(" — {}", d))
                    .unwrap_or_default();
                let usage = def
                    .usage
                    .as_deref()
                    .unwrap_or(name.as_str());
                format!("  {} {}", usage, desc)
            })
            .collect();

        if !skill_list.is_empty() {
            return Ok(format!(
                "I don't understand \"{}\". Available skills:\n{}",
                text,
                skill_list.join("\n")
            ));
        }
    }

    bail!("no matching skill for: {}", text)
}

// ─── Telegram API helpers ───

async fn tg_request(
    client: &reqwest::Client,
    token: &str,
    method: &str,
    params: &Value,
) -> Result<Value> {
    let url = format!("{}/bot{}/{}", TELEGRAM_API, token, method);
    let resp = client
        .post(&url)
        .json(params)
        .send()
        .await
        .context("telegram API request failed")?;

    let body: Value = resp.json().await.context("invalid telegram response")?;
    if body["ok"].as_bool() != Some(true) {
        bail!(
            "telegram API error: {}",
            body["description"].as_str().unwrap_or("unknown")
        );
    }
    Ok(body)
}

async fn tg_send(
    client: &reqwest::Client,
    token: &str,
    chat_id: i64,
    text: &str,
) -> Result<()> {
    // Telegram message limit is 4096 chars
    let truncated = if text.len() > 4000 {
        format!("{}...\n(truncated)", &text[..4000])
    } else {
        text.to_string()
    };

    tg_request(
        client,
        token,
        "sendMessage",
        &json!({
            "chat_id": chat_id,
            "text": truncated,
            "parse_mode": "Markdown",
        }),
    )
    .await?;
    Ok(())
}

// ─── Helpers (replicated from main.rs since they're not pub) ───

fn exec_skill_script(
    script: &Path,
    args: &str,
    working_dir: &Path,
    env: &[(String, String)],
) -> Result<(i32, String, String)> {
    let mut cmd = std::process::Command::new("bash");
    cmd.arg(script);
    if !args.is_empty() {
        for arg in args.split_whitespace() {
            cmd.arg(arg);
        }
    }
    cmd.current_dir(working_dir);
    for (k, v) in env {
        cmd.env(k, v);
    }
    let output = cmd
        .output()
        .with_context(|| format!("failed to execute: {}", script.display()))?;
    Ok((
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    ))
}

fn load_scoped_env(
    inst_dir: &Path,
    skill_name: Option<&str>,
) -> Result<Vec<(String, String)>> {
    let all_env = load_vault_env()?;
    if all_env.is_empty() {
        return Ok(Vec::new());
    }

    let agentfile = inst_dir.join("Agentfile.toml");
    if !agentfile.exists() {
        return Ok(all_env);
    }

    let spec = AgentfileSpec::load(inst_dir).unwrap_or_else(|_| empty_spec());

    if let Some(sname) = skill_name {
        if let Some(skill_def) = spec.skills.get(sname) {
            if let Some(skill_env) = &skill_def.env {
                let allowed: std::collections::HashSet<String> =
                    skill_env.iter().cloned().collect();
                return Ok(all_env
                    .into_iter()
                    .filter(|(k, _)| allowed.contains(k))
                    .collect());
            }
        }
    }

    let allowed: std::collections::HashSet<String> = match &spec.env {
        Some(env_section) => {
            let mut set = std::collections::HashSet::new();
            for k in &env_section.required {
                set.insert(k.clone());
            }
            for k in &env_section.optional {
                set.insert(k.clone());
            }
            set
        }
        None => return Ok(Vec::new()),
    };

    Ok(all_env
        .into_iter()
        .filter(|(k, _)| allowed.contains(k))
        .collect())
}

fn empty_spec() -> AgentfileSpec {
    use crate::agents::agentfile::*;
    use std::collections::HashMap;
    AgentfileSpec {
        agent: AgentMeta {
            name: String::new(),
            version: String::new(),
            description: None,
            author: None,
        },
        persona: None,
        skills: HashMap::new(),
        seed: None,
        env: None,
        soul: None,
    }
}

fn load_vault_env() -> Result<Vec<(String, String)>> {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let aide_home = std::path::PathBuf::from(home).join(".aide");
    let vault_path = aide_home.join("vault.age");
    if !vault_path.exists() {
        return Ok(Vec::new());
    }
    let identity_path = aide_home.join("vault.key");
    if !identity_path.exists() {
        return Ok(Vec::new());
    }

    let output = std::process::Command::new("age")
        .args(["-d", "-i"])
        .arg(&identity_path)
        .arg(&vault_path)
        .output()?;

    if !output.status.success() {
        return Ok(Vec::new());
    }

    let content = String::from_utf8_lossy(&output.stdout);
    let mut vars = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").unwrap_or(line);
        if let Some((key, val)) = line.split_once('=') {
            let val = val.trim_matches('"').trim_matches('\'');
            vars.push((key.to_string(), val.to_string()));
        }
    }
    Ok(vars)
}
