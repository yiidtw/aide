use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use tracing::{error, info, warn};

use crate::agents::agentfile::AgentfileSpec;
use crate::agents::instance::InstanceManager;

const TELEGRAM_API: &str = "https://api.telegram.org";

/// Maximum Telegram message length.
const TG_MAX_LEN: usize = 4096;

/// Send a request to the Telegram Bot API.
async fn tg_request(
    client: &reqwest::Client,
    token: &str,
    method: &str,
    params: &serde_json::Value,
) -> Result<serde_json::Value> {
    let url = format!("{}/bot{}/{}", TELEGRAM_API, token, method);
    let resp = client
        .post(&url)
        .json(params)
        .send()
        .await
        .with_context(|| format!("Telegram API request failed: {}", method))?;

    let body: serde_json::Value = resp
        .json()
        .await
        .with_context(|| format!("Telegram API response parse failed: {}", method))?;

    if body.get("ok").and_then(|v| v.as_bool()) != Some(true) {
        let desc = body
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown error");
        anyhow::bail!("Telegram API error ({}): {}", method, desc);
    }

    Ok(body)
}

/// Send a text message to a Telegram chat, truncating to the API limit.
async fn tg_send(
    client: &reqwest::Client,
    token: &str,
    chat_id: i64,
    text: &str,
) -> Result<()> {
    let truncated: String = if text.len() > TG_MAX_LEN {
        let mut t: String = text.chars().take(TG_MAX_LEN - 20).collect();
        t.push_str("\n...(truncated)");
        t
    } else {
        text.to_string()
    };

    let params = serde_json::json!({
        "chat_id": chat_id,
        "text": truncated,
    });

    tg_request(client, token, "sendMessage", &params).await?;
    Ok(())
}

/// Main Telegram bot loop. Long-polls for updates and dispatches messages.
pub async fn run_telegram_bot(data_dir: &str, instance: &str, token: &str) -> Result<()> {
    let client = reqwest::Client::new();
    let mgr = InstanceManager::new(data_dir);
    let inst_dir = mgr.base_dir().join(instance);
    let mut offset: i64 = 0;

    info!(instance = %instance, "telegram bot polling started");
    let _ = mgr.append_log(instance, "telegram: bot started");

    loop {
        let params = serde_json::json!({
            "offset": offset,
            "timeout": 30,
        });

        let resp = match tg_request(&client, token, "getUpdates", &params).await {
            Ok(r) => r,
            Err(e) => {
                warn!(instance = %instance, error = %e, "telegram getUpdates failed, retrying in 5s");
                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                continue;
            }
        };

        let updates = match resp.get("result").and_then(|v| v.as_array()) {
            Some(arr) => arr.clone(),
            None => continue,
        };

        for update in &updates {
            // Advance offset past this update
            if let Some(uid) = update.get("update_id").and_then(|v| v.as_i64()) {
                offset = uid + 1;
            }

            let message = match update.get("message") {
                Some(m) => m,
                None => continue,
            };

            let chat_id = match message
                .get("chat")
                .and_then(|c| c.get("id"))
                .and_then(|v| v.as_i64())
            {
                Some(id) => id,
                None => continue,
            };

            let text = match message.get("text").and_then(|v| v.as_str()) {
                Some(t) => t.to_string(),
                None => continue,
            };

            let from = message
                .get("from")
                .and_then(|f| f.get("username"))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");

            info!(
                instance = %instance,
                from = %from,
                text = %text,
                "telegram message received"
            );
            let _ = mgr.append_log(
                instance,
                &format!("telegram-msg: from={} text={}", from, &text),
            );

            // Handle /start command
            if text.starts_with("/start") {
                let welcome = format_welcome(instance, &inst_dir);
                if let Err(e) = tg_send(&client, token, chat_id, &welcome).await {
                    error!(error = %e, "telegram send failed");
                }
                continue;
            }

            // Parse: first word is skill name, rest are args
            let parts: Vec<&str> = text.splitn(2, char::is_whitespace).collect();
            let skill_name = parts[0].trim_start_matches('/');
            let skill_args = if parts.len() > 1 { parts[1] } else { "" };

            // Check if skill exists
            let script = inst_dir
                .join("skills")
                .join(format!("{}.sh", skill_name));

            if !script.exists() {
                // Skill not found — try claude -p for natural language
                info!(instance = %instance, text = %text, "no matching skill, trying claude -p");
                let _ = mgr.append_log(instance, &format!("telegram-prompt: {}", text));

                let reply = match try_claude_prompt(instance, &text, &inst_dir) {
                    Some(output) => {
                        let _ = mgr.append_log(instance, &format!("telegram-prompt-result: ok"));
                        output
                    }
                    None => {
                        format_skill_not_found(instance, skill_name, &inst_dir)
                    }
                };
                if let Err(e) = tg_send(&client, token, chat_id, &reply).await {
                    error!(error = %e, "telegram send failed");
                }
                continue;
            }

            // Execute skill
            let _ = mgr.append_log(
                instance,
                &format!("telegram-exec: {} {}", skill_name, skill_args),
            );

            let result = exec_skill(&inst_dir, skill_name, skill_args, data_dir);

            let reply = match result {
                Ok((exit_code, stdout, stderr)) => {
                    let status = if exit_code == 0 { "ok" } else { "fail" };
                    let _ = mgr.append_log(
                        instance,
                        &format!(
                            "telegram-exec-result: {} -> {} (exit {})",
                            skill_name, status, exit_code
                        ),
                    );

                    let mut out = String::new();
                    if !stdout.trim().is_empty() {
                        out.push_str(&stdout);
                    }
                    if !stderr.trim().is_empty() {
                        if !out.is_empty() && !out.ends_with('\n') {
                            out.push('\n');
                        }
                        out.push_str("[stderr]\n");
                        out.push_str(&stderr);
                    }
                    if exit_code != 0 {
                        if !out.is_empty() && !out.ends_with('\n') {
                            out.push('\n');
                        }
                        out.push_str(&format!("[exit code: {}]", exit_code));
                    }
                    if out.trim().is_empty() {
                        "(no output)".to_string()
                    } else {
                        out
                    }
                }
                Err(e) => {
                    let _ = mgr.append_log(
                        instance,
                        &format!("telegram-exec-error: {} -> {}", skill_name, e),
                    );
                    format!("Error: {}", e)
                }
            };

            if let Err(e) = tg_send(&client, token, chat_id, &reply).await {
                error!(error = %e, "telegram send failed");
            }
        }
    }
}

/// Spawn the Telegram bot as a background tokio task.
pub fn spawn_telegram_bot(data_dir: String, instance: String, token: String) {
    tokio::spawn(async move {
        if let Err(e) = run_telegram_bot(&data_dir, &instance, &token).await {
            error!(
                instance = %instance,
                error = %e,
                "telegram bot exited with error"
            );
        }
    });
}

/// Try to use claude -p to interpret natural language and run matching skills.
fn try_claude_prompt(instance: &str, query: &str, inst_dir: &Path) -> Option<String> {
    // Read persona
    let persona = std::fs::read_to_string(inst_dir.join("persona.md")).unwrap_or_default();

    // Read skill catalog
    let skill_info = AgentfileSpec::load(inst_dir)
        .ok()
        .map(|spec| spec.format_help(instance))
        .unwrap_or_default();

    let prompt = format!(
        "You are an agent assistant. Given the persona and skills below, \
         answer the user's query by deciding which skill to call.\n\
         Respond with EXEC: <skill_name> [args] or answer directly if no skill matches.\n\n\
         ## Persona\n{}\n\n## Skills\n{}\n\n## Query\n{}",
        persona, skill_info, query
    );

    // Try claude -p
    let output = std::process::Command::new("claude")
        .arg("-p")
        .arg(&prompt)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let response = String::from_utf8_lossy(&output.stdout).to_string();

    // Check for EXEC: lines and run them
    for line in response.lines() {
        let line = line.trim();
        if let Some(cmd) = line.strip_prefix("EXEC:") {
            let cmd = cmd.trim();
            let parts: Vec<&str> = cmd.splitn(2, ' ').collect();
            let skill_name = parts[0];
            let args = if parts.len() > 1 { parts[1] } else { "" };

            let script = inst_dir.join("skills").join(format!("{}.sh", skill_name));
            if script.exists() {
                // Load vault env
                let env = load_vault_env().unwrap_or_default();
                match exec_skill_raw(&script, args, inst_dir, &env) {
                    Ok((_, stdout, _)) => return Some(stdout),
                    Err(e) => return Some(format!("Error running {}: {}", skill_name, e)),
                }
            }
        }
    }

    // No EXEC: found — return raw claude response
    Some(response)
}

fn exec_skill_raw(
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
    let output = cmd.output()
        .with_context(|| format!("failed to execute: {}", script.display()))?;
    Ok((
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    ))
}

/// Execute a skill script, returning (exit_code, stdout, stderr).
fn exec_skill(
    inst_dir: &PathBuf,
    skill_name: &str,
    args: &str,
    data_dir: &str,
) -> Result<(i32, String, String)> {
    let script = inst_dir
        .join("skills")
        .join(format!("{}.sh", skill_name));

    if !script.exists() {
        anyhow::bail!("skill script not found: {}", script.display());
    }

    // Load vault env
    let vault_env = load_vault_env().unwrap_or_default();

    let mut cmd = std::process::Command::new("bash");
    cmd.arg(&script);
    if !args.is_empty() {
        for arg in args.split_whitespace() {
            cmd.arg(arg);
        }
    }
    cmd.current_dir(inst_dir);
    cmd.env("AIDE_INSTANCE_DIR", inst_dir);
    cmd.env("AIDE_DATA_DIR", data_dir);

    for (k, v) in &vault_env {
        cmd.env(k, v);
    }

    let output = cmd
        .output()
        .with_context(|| format!("failed to execute skill: {}", script.display()))?;

    Ok((
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    ))
}

/// Format a welcome message listing available skills.
fn format_welcome(instance: &str, inst_dir: &PathBuf) -> String {
    let mut msg = format!("Welcome to {} agent!\n\n", instance);

    if let Ok(spec) = AgentfileSpec::load(inst_dir) {
        if let Some(desc) = &spec.agent.description {
            msg.push_str(&format!("{}\n\n", desc));
        }
        if !spec.skills.is_empty() {
            msg.push_str("Available skills:\n");
            let mut names: Vec<&String> = spec.skills.keys().collect();
            names.sort();
            for name in names {
                let skill = &spec.skills[name];
                let desc = skill
                    .description
                    .as_deref()
                    .map(|d| format!(" - {}", d))
                    .unwrap_or_default();
                msg.push_str(&format!("  /{}{}\n", name, desc));
            }
        }
    }

    msg.push_str("\nSend a skill name followed by arguments to execute it.");
    msg
}

/// Format a "skill not found" reply with the list of available skills.
fn format_skill_not_found(_instance: &str, skill_name: &str, inst_dir: &PathBuf) -> String {
    let mut msg = format!("Unknown skill: {}\n\n", skill_name);

    if let Ok(spec) = AgentfileSpec::load(inst_dir) {
        if !spec.skills.is_empty() {
            msg.push_str("Available skills:\n");
            let mut names: Vec<&String> = spec.skills.keys().collect();
            names.sort();
            for name in names {
                msg.push_str(&format!("  /{}\n", name));
            }
        } else {
            msg.push_str("This agent has no skills configured.");
        }
    }

    msg
}

/// Load vault environment variables by decrypting vault.age with vault.key.
fn load_vault_env() -> Result<Vec<(String, String)>> {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let aide_home = PathBuf::from(home).join(".aide");

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
