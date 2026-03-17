use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

use crate::agents::agentfile::AgentfileSpec;
use crate::agents::instance::InstanceManager;

/// Run the MCP stdio server (JSON-RPC 2.0 over stdin/stdout).
///
/// Reads newline-delimited JSON-RPC messages from stdin,
/// dispatches them, and writes responses to stdout.
pub fn run_mcp_server(data_dir: &str) -> Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let reader = stdin.lock();
    let mut writer = stdout.lock();

    let mgr = InstanceManager::new(data_dir);

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break, // stdin closed
        };

        let line = line.trim().to_string();
        if line.is_empty() {
            continue;
        }

        let msg: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                let err_resp = json!({
                    "jsonrpc": "2.0",
                    "id": null,
                    "error": {
                        "code": -32700,
                        "message": format!("Parse error: {}", e)
                    }
                });
                write_response(&mut writer, &err_resp)?;
                continue;
            }
        };

        let id = msg.get("id").cloned();
        let method = msg.get("method").and_then(|m| m.as_str()).unwrap_or("");

        // Notifications (no id) don't get a response
        if id.is_none() {
            // e.g. "notifications/initialized" — just ignore
            continue;
        }

        let id = id.unwrap();

        let response = match method {
            "initialize" => handle_initialize(id.clone()),
            "tools/list" => handle_tools_list(id.clone()),
            "tools/call" => handle_tools_call(id.clone(), &msg, &mgr),
            "ping" => json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {}
            }),
            _ => json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": {
                    "code": -32601,
                    "message": format!("Method not found: {}", method)
                }
            }),
        };

        write_response(&mut writer, &response)?;
    }

    Ok(())
}

fn write_response(writer: &mut impl Write, response: &Value) -> Result<()> {
    let s = serde_json::to_string(response)?;
    writeln!(writer, "{}", s)?;
    writer.flush()?;
    Ok(())
}

fn handle_initialize(id: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "protocolVersion": "2024-11-05",
            "capabilities": {
                "tools": {}
            },
            "serverInfo": {
                "name": "aide.sh",
                "version": "0.1.0"
            }
        }
    })
}

fn handle_tools_list(id: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "tools": [
                {
                    "name": "aide_list",
                    "description": "List all running agent instances and their available skills",
                    "inputSchema": {
                        "type": "object",
                        "properties": {}
                    }
                },
                {
                    "name": "aide_exec",
                    "description": "Execute a skill on an agent instance",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "instance": {
                                "type": "string",
                                "description": "Agent instance name"
                            },
                            "skill": {
                                "type": "string",
                                "description": "Skill name to execute"
                            },
                            "args": {
                                "type": "string",
                                "description": "Arguments to pass to the skill"
                            }
                        },
                        "required": ["instance", "skill"]
                    }
                },
                {
                    "name": "aide_logs",
                    "description": "Read recent logs from an agent instance",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "instance": {
                                "type": "string",
                                "description": "Agent instance name"
                            },
                            "lines": {
                                "type": "number",
                                "description": "Number of log lines to return (default 50)"
                            }
                        },
                        "required": ["instance"]
                    }
                }
            ]
        }
    })
}

fn handle_tools_call(id: Value, msg: &Value, mgr: &InstanceManager) -> Value {
    let params = msg.get("params").cloned().unwrap_or(json!({}));
    let tool_name = params.get("name").and_then(|n| n.as_str()).unwrap_or("");
    let arguments = params.get("arguments").cloned().unwrap_or(json!({}));

    match tool_name {
        "aide_list" => tool_aide_list(id, mgr),
        "aide_exec" => tool_aide_exec(id, &arguments, mgr),
        "aide_logs" => tool_aide_logs(id, &arguments, mgr),
        _ => tool_error(id, &format!("Unknown tool: {}", tool_name)),
    }
}

fn tool_result(id: Value, text: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "content": [{ "type": "text", "text": text }]
        }
    })
}

fn tool_error(id: Value, text: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "content": [{ "type": "text", "text": text }],
            "isError": true
        }
    })
}

// ─── Tool: aide_list ────────────────────────────────────────────

fn tool_aide_list(id: Value, mgr: &InstanceManager) -> Value {
    let instances = match mgr.list() {
        Ok(v) => v,
        Err(e) => return tool_error(id, &format!("Error listing instances: {}", e)),
    };

    if instances.is_empty() {
        return tool_result(id, "No running instances.");
    }

    let mut output = String::new();
    for inst in &instances {
        output.push_str(&format!(
            "instance: {}  type: {}  status: {}  email: {}\n",
            inst.name, inst.agent_type, inst.status, inst.email
        ));

        // Try to load Agentfile.toml and list skills
        let inst_dir = mgr.base_dir().join(&inst.name);
        if let Ok(spec) = AgentfileSpec::load(&inst_dir) {
            if !spec.skills.is_empty() {
                let mut skill_names: Vec<&String> = spec.skills.keys().collect();
                skill_names.sort();
                for name in skill_names {
                    let skill = &spec.skills[name];
                    let kind = if skill.script.is_some() {
                        "script"
                    } else if skill.prompt.is_some() {
                        "prompt"
                    } else {
                        "unknown"
                    };
                    let desc = skill
                        .description
                        .as_deref()
                        .map(|d| format!(" — {}", d))
                        .unwrap_or_default();
                    output.push_str(&format!("  skill: {} ({}){}\n", name, kind, desc));
                }
            }
        }
    }

    tool_result(id, output.trim_end())
}

// ─── Tool: aide_exec ────────────────────────────────────────────

fn tool_aide_exec(id: Value, args: &Value, mgr: &InstanceManager) -> Value {
    let instance = match args.get("instance").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => return tool_error(id, "Error: missing required parameter 'instance'"),
    };
    let skill = match args.get("skill").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => return tool_error(id, "Error: missing required parameter 'skill'"),
    };
    let skill_args = args
        .get("args")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    // Verify instance exists
    match mgr.get(instance) {
        Ok(Some(_)) => {}
        Ok(None) => return tool_error(id, &format!("Error: no such instance: {}", instance)),
        Err(e) => return tool_error(id, &format!("Error: {}", e)),
    }

    let inst_dir = mgr.base_dir().join(instance);

    // Log the exec
    let _ = mgr.append_log(instance, &format!("mcp-exec: {} {}", skill, skill_args));

    // Load scoped env
    let scoped_env = match load_scoped_env(&inst_dir, Some(skill)) {
        Ok(v) => v,
        Err(e) => return tool_error(id, &format!("Error loading env: {}", e)),
    };

    // Find the skill script
    let local_script = inst_dir.join("skills").join(format!("{}.sh", skill));
    if !local_script.exists() {
        return tool_error(
            id,
            &format!("Error: skill script not found: {}", local_script.display()),
        );
    }

    // Execute
    let (exit_code, stdout, stderr) =
        match exec_skill_script(&local_script, skill_args, &inst_dir, &scoped_env) {
            Ok(v) => v,
            Err(e) => return tool_error(id, &format!("Error executing skill: {}", e)),
        };

    // Log result
    let status_msg = if exit_code == 0 { "ok" } else { "FAILED" };
    let _ = mgr.append_log(
        instance,
        &format!(
            "mcp-exec-result: {} → {} (exit {})",
            skill, status_msg, exit_code
        ),
    );

    // Build output
    let mut output = String::new();
    if !stdout.is_empty() {
        output.push_str(&stdout);
    }
    if !stderr.is_empty() {
        if !output.is_empty() && !output.ends_with('\n') {
            output.push('\n');
        }
        output.push_str("[stderr]\n");
        output.push_str(&stderr);
    }

    if exit_code != 0 {
        if !output.is_empty() && !output.ends_with('\n') {
            output.push('\n');
        }
        output.push_str(&format!("[exit code: {}]", exit_code));
        return tool_error(id, &output);
    }

    tool_result(id, &output)
}

// ─── Tool: aide_logs ────────────────────────────────────────────

fn tool_aide_logs(id: Value, args: &Value, mgr: &InstanceManager) -> Value {
    let instance = match args.get("instance").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => return tool_error(id, "Error: missing required parameter 'instance'"),
    };
    let lines = args
        .get("lines")
        .and_then(|v| v.as_u64())
        .unwrap_or(50) as usize;

    // Verify instance exists
    match mgr.get(instance) {
        Ok(Some(_)) => {}
        Ok(None) => return tool_error(id, &format!("Error: no such instance: {}", instance)),
        Err(e) => return tool_error(id, &format!("Error: {}", e)),
    }

    let log_lines = match mgr.read_logs(instance, lines) {
        Ok(v) => v,
        Err(e) => return tool_error(id, &format!("Error reading logs: {}", e)),
    };

    if log_lines.is_empty() {
        return tool_result(id, "(no logs)");
    }

    tool_result(id, &log_lines.join("\n"))
}

// ─── Helpers (replicated from main.rs since they are not pub) ───

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
        .with_context(|| format!("failed to execute script: {}", script.display()))?;

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
        return Ok(all_env); // Legacy: no Agentfile = inject all
    }

    let spec = AgentfileSpec::load(inst_dir).unwrap_or_else(|_| empty_spec());

    // Tier 1: per-skill env (if skill has its own env list, use ONLY those)
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

    // Tier 2: per-agent env ([env] section)
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
    AgentfileSpec {
        agent: crate::agents::agentfile::AgentMeta {
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
        expose: None,
    }
}

fn aide_home() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".aide")
}

fn load_vault_env() -> Result<Vec<(String, String)>> {
    let vault_path = aide_home().join("vault.age");
    if !vault_path.exists() {
        return Ok(Vec::new());
    }
    let identity_path = aide_home().join("vault.key");
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
