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
                    "description": "List all agent instances, their skills, and status. Use this first to see what agents are available. If no agents exist, suggest using aide_create to make one.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {}
                    }
                },
                {
                    "name": "aide_exec",
                    "description": "Execute a skill on an agent instance. Run aide_list first to see available instances and skills. Example: instance='ntu.yiidtw', skill='cool', args='courses'",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "instance": {
                                "type": "string",
                                "description": "Agent instance name (from aide_list)"
                            },
                            "skill": {
                                "type": "string",
                                "description": "Skill name to execute (from aide_list)"
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
                    "name": "aide_create",
                    "description": "Create a new agent instance. Creates the occupation/ (skills, persona, knowledge) and cognition/ (memory, logs) directory structure. After creation, add skills as .ts files in occupation/skills/. The agent needs no LLM by default — set [soul] prefer in Agentfile.toml if LLM is needed.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "name": {
                                "type": "string",
                                "description": "Agent name (e.g. 'easychair'). Instance will be named '<name>.yiidtw'"
                            },
                            "description": {
                                "type": "string",
                                "description": "One-line description of what this agent does"
                            },
                            "skills": {
                                "type": "array",
                                "items": { "type": "string" },
                                "description": "List of skill names to scaffold (e.g. ['review', 'submit', 'status'])"
                            }
                        },
                        "required": ["name", "description"]
                    }
                },
                {
                    "name": "aide_logs",
                    "description": "Read recent logs from an agent instance. Shows execution history, errors, and GITAW activity.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "instance": {
                                "type": "string",
                                "description": "Agent instance name (from aide_list)"
                            },
                            "lines": {
                                "type": "number",
                                "description": "Number of log lines to return (default 50)"
                            }
                        },
                        "required": ["instance"]
                    }
                },
                {
                    "name": "aide_commit",
                    "description": "Commit and push changes for an agent instance to its private GitHub repo. Use this after modifying an agent's skills, memory, or knowledge files. Returns a sanity check confirming changes reached the remote.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "instance": {
                                "type": "string",
                                "description": "Agent instance name (from aide_list)"
                            },
                            "message": {
                                "type": "string",
                                "description": "Commit message describing the changes"
                            }
                        },
                        "required": ["instance", "message"]
                    }
                },
                {
                    "name": "aide_commit_all",
                    "description": "Commit and push ALL dirty agent instances to their private GitHub repos. Use this before ending a session to ensure no changes are lost. Returns per-instance sanity checks.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {}
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
        "aide_create" => tool_aide_create(id, &arguments, mgr),
        "aide_logs" => tool_aide_logs(id, &arguments, mgr),
        "aide_commit" => tool_aide_commit(id, &arguments, mgr),
        "aide_commit_all" => tool_aide_commit_all(id, mgr),
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

    // Find the skill script (try occupation/skills/ first, then skills/)
    let local_script = match find_skill_script(&inst_dir, skill) {
        Some(s) => s,
        None => return tool_error(
            id,
            &format!("Error: skill script not found for '{}'", skill),
        ),
    };

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

    // Auto-commit if instance is a git repo (fire-and-forget)
    if let Some(commit_summary) = auto_commit_instance(&inst_dir, &format!("exec: {}", skill)) {
        if !output.is_empty() && !output.ends_with('\n') {
            output.push('\n');
        }
        output.push_str(&format!("[auto-commit] {}", commit_summary.lines().next().unwrap_or("committed")));
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

// ─── Tool: aide_create ──────────────────────────────────────────

fn tool_aide_create(id: Value, args: &Value, mgr: &InstanceManager) -> Value {
    let name = match args.get("name").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => return tool_error(id, "Error: missing required parameter 'name'"),
    };
    let description = args.get("description").and_then(|v| v.as_str()).unwrap_or("An aide agent");
    let skills: Vec<String> = args.get("skills")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();

    // Derive instance name
    let username = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "yiidtw".to_string());
    let instance_name = format!("{}.{}", name, username);

    // Check if already exists
    match mgr.get(&instance_name) {
        Ok(Some(_)) => return tool_error(id, &format!("Instance '{}' already exists. Use aide_exec to run skills on it.", instance_name)),
        _ => {}
    }

    let inst_dir = mgr.base_dir().join(&instance_name);

    // Create occupation/cognition structure
    let dirs = [
        "occupation/skills",
        "occupation/knowledge",
        "cognition/memory",
        "cognition/logs",
    ];
    for dir in &dirs {
        if let Err(e) = std::fs::create_dir_all(inst_dir.join(dir)) {
            return tool_error(id, &format!("Error creating directory: {}", e));
        }
    }

    // Write Agentfile.toml
    let mut agentfile = format!(r#"[agent]
name = "{name}"
version = "0.1.0"
description = "{description}"
author = "{username}"

[persona]
file = "persona.md"

[soul]
prefer = "none"
"#);

    for skill in &skills {
        agentfile.push_str(&format!(r#"
[skills.{skill}]
script = "skills/{skill}.ts"
description = "{skill} skill"
"#));
    }

    agentfile.push_str(r#"
[knowledge]
dir = "knowledge/"
"#);

    let _ = std::fs::write(inst_dir.join("occupation/Agentfile.toml"), &agentfile);

    // Write persona.md
    let persona = format!("# {}\n\n{}\n\n## Skills\n\n{}\n",
        name, description,
        if skills.is_empty() { "No skills yet. Add .ts files to occupation/skills/.".to_string() }
        else { skills.iter().map(|s| format!("- **{}**", s)).collect::<Vec<_>>().join("\n") }
    );
    let _ = std::fs::write(inst_dir.join("occupation/persona.md"), &persona);

    // Write skill stubs
    for skill in &skills {
        let stub = format!(r#"// {skill} — {description}
// usage: {skill} [args...]

const args = process.argv.slice(2);
console.log(`{skill}: ${{args.join(" ") || "(no args)"}}`);
console.log("TODO: implement {skill} skill");
"#);
        let _ = std::fs::write(inst_dir.join(format!("occupation/skills/{}.ts", skill)), stub);
    }

    // Write instance.toml
    let uuid = format!("{:08x}-{:04x}-4{:03x}-{:04x}-{:012x}",
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs() as u32,
        (std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().subsec_nanos() >> 16) as u16,
        (std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().subsec_nanos() & 0xFFF) as u16,
        0x8000u16 | (std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().subsec_nanos() as u16 & 0x3FFF),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos() as u64 & 0xFFFFFFFFFFFF,
    );
    let hostname = hostname::get()
        .map(|h| h.to_string_lossy().to_string())
        .unwrap_or_else(|_| "unknown".to_string());

    let instance_toml = format!(r#"name = "{instance_name}"
agent_type = "{name}"
created_at = "{}"
email = "{name}@aide.sh"
role = "{description}"
domains = []
uuid = "{uuid}"
machine_id = "{hostname}"
"#, chrono::Utc::now().to_rfc3339());
    let _ = std::fs::write(inst_dir.join("cognition/instance.toml"), &instance_toml);

    // Write .aideignore
    let _ = std::fs::write(inst_dir.join(".aideignore"), "cognition/\n");

    // Log
    let _ = mgr.append_log(&instance_name, &format!("created via MCP (skills: {:?})", skills));

    // Build response
    let mut output = format!("Created agent: {}\n\nStructure:\n  occupation/\n    Agentfile.toml\n    persona.md\n    skills/\n    knowledge/\n  cognition/\n    instance.toml\n    memory/\n    logs/\n", instance_name);

    if !skills.is_empty() {
        output.push_str(&format!("\nSkill stubs created:\n"));
        for skill in &skills {
            output.push_str(&format!("  occupation/skills/{}.ts (TODO: implement)\n", skill));
        }
    }

    output.push_str(&format!("\nNext steps:\n"));
    output.push_str(&format!("  1. Edit skill .ts files in {}/occupation/skills/\n", inst_dir.display()));
    output.push_str(&format!("  2. If skills need secrets: aide vault set KEY (user runs in terminal)\n"));
    output.push_str(&format!("  3. Test: aide exec {} <skill>\n", instance_name));

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

// ─── Tool: aide_commit ──────────────────────────────────────────

fn tool_aide_commit(id: Value, args: &Value, mgr: &InstanceManager) -> Value {
    let instance = match args.get("instance").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => return tool_error(id, "Error: missing required parameter 'instance'"),
    };
    let message = match args.get("message").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => return tool_error(id, "Error: missing required parameter 'message'"),
    };

    // Verify instance exists
    match mgr.get(instance) {
        Ok(Some(_)) => {}
        Ok(None) => return tool_error(id, &format!("Error: no such instance: {}", instance)),
        Err(e) => return tool_error(id, &format!("Error: {}", e)),
    }

    let inst_dir = mgr.base_dir().join(instance);

    if !inst_dir.join(".git").exists() {
        return tool_error(
            id,
            &format!("Instance '{}' is not a git repo. Run: aide deploy --github {}", instance, instance),
        );
    }

    match auto_commit_instance(&inst_dir, message) {
        Some(summary) => {
            let _ = mgr.append_log(instance, &format!("mcp-commit: {}", message));
            tool_result(id, &summary)
        }
        None => tool_result(id, "nothing to commit (no changes)"),
    }
}

// ─── Tool: aide_commit_all ──────────────────────────────────────

fn tool_aide_commit_all(id: Value, mgr: &InstanceManager) -> Value {
    let instances = match mgr.list() {
        Ok(v) => v,
        Err(e) => return tool_error(id, &format!("Error listing instances: {}", e)),
    };

    let mut output = String::new();
    let mut committed = 0usize;
    let mut skipped = 0usize;

    for inst in &instances {
        let inst_dir = mgr.base_dir().join(&inst.name);
        if !inst_dir.join(".git").exists() {
            skipped += 1;
            continue;
        }

        match auto_commit_instance(&inst_dir, "auto-commit: session sync") {
            Some(summary) => {
                output.push_str(&format!("{}:\n{}\n\n", inst.name, summary));
                let _ = mgr.append_log(&inst.name, "mcp-commit-all: session sync");
                committed += 1;
            }
            None => {
                // no changes, skip silently
            }
        }
    }

    if committed == 0 {
        tool_result(id, &format!("all clean — {} instances checked, {} skipped (not git-backed)", instances.len() - skipped, skipped))
    } else {
        output.push_str(&format!("total: {} committed, {} skipped", committed, skipped));
        tool_result(id, output.trim_end())
    }
}

// ─── Git helpers ────────────────────────────────────────────────

/// Auto-commit and push an instance directory if it's a git repo.
/// Returns a summary string on success, or None if no changes / not a git repo.
fn auto_commit_instance(inst_dir: &std::path::Path, message: &str) -> Option<String> {
    if !inst_dir.join(".git").exists() {
        return None;
    }

    let git_output = |args: &[&str]| -> std::result::Result<String, String> {
        let output = std::process::Command::new("git")
            .args(args)
            .current_dir(inst_dir)
            .output()
            .map_err(|e| e.to_string())?;
        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
        }
    };

    let git_ok = |args: &[&str]| -> bool {
        std::process::Command::new("git")
            .args(args)
            .current_dir(inst_dir)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    };

    git_ok(&["add", "-A"]);

    if git_ok(&["diff", "--cached", "--quiet"]) {
        return None;
    }

    let diff_stat = git_output(&["diff", "--cached", "--name-only"]).unwrap_or_default();
    let mut occ_count = 0usize;
    let mut cog_count = 0usize;
    let mut other_count = 0usize;
    for line in diff_stat.lines() {
        if line.starts_with("occupation/") {
            occ_count += 1;
        } else if line.starts_with("cognition/") {
            cog_count += 1;
        } else {
            other_count += 1;
        }
    }
    let total = occ_count + cog_count + other_count;

    if !git_ok(&["commit", "-m", message]) {
        return None;
    }

    let push_ok = git_ok(&["push"]);

    let sanity = if push_ok {
        git_ok(&["fetch", "origin", "--quiet"]);
        let local_head = git_output(&["rev-parse", "HEAD"]).unwrap_or_default();
        let remote_head = git_output(&["rev-parse", "origin/main"]).unwrap_or_default();
        if !local_head.is_empty() && local_head == remote_head {
            let short = &local_head[..7.min(local_head.len())];
            format!("sanity: HEAD == origin/main ({})", short)
        } else {
            format!("sanity: MISMATCH local={} remote={}",
                &local_head[..7.min(local_head.len())],
                &remote_head[..7.min(remote_head.len())])
        }
    } else {
        "sanity: push failed, remote not updated".to_string()
    };

    Some(format!(
        "committed: {} files ({} occupation, {} cognition{})\npushed: {}\n{}",
        total, occ_count, cog_count,
        if other_count > 0 { format!(", {} other", other_count) } else { String::new() },
        if push_ok { "ok" } else { "FAILED" },
        sanity,
    ))
}

// ─── Helpers (replicated from main.rs since they are not pub) ───

/// Find a skill script, trying occupation/skills/ first, then skills/.
fn find_skill_script(inst_dir: &Path, skill_name: &str) -> Option<PathBuf> {
    for dir in &["occupation/skills", "skills"] {
        for ext in &["ts", "sh"] {
            let path = inst_dir.join(dir).join(format!("{}.{}", skill_name, ext));
            if path.exists() {
                return Some(path);
            }
        }
    }
    None
}

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

    // Check for Agentfile.toml (new: occupation/, old: root)
    let new_agentfile = inst_dir.join("occupation/Agentfile.toml");
    let old_agentfile = inst_dir.join("Agentfile.toml");
    if !new_agentfile.exists() && !old_agentfile.exists() {
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
        knowledge: None,
        env: None,
        soul: None,
        expose: None,
        limits: None,
    }
}

fn aide_home() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".aide")
}

fn load_vault_env() -> Result<Vec<(String, String)>> {
    // Try vault repo first, then legacy ~/.aide/vault.age
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let vault_repo_path = std::path::PathBuf::from(&home).join("claude_projects/aide-vault/vault.age");
    let legacy_path = aide_home().join("vault.age");
    let vault_path = if vault_repo_path.exists() { vault_repo_path } else { legacy_path };
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
