//! MCP stdio server — JSON-RPC 2.0 over stdin/stdout.
//!
//! Exposes aide commands as LLM-callable tools.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};

/// Run the MCP stdio server loop.
pub fn serve() -> Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();

    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }

        let request: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                write_response(&stdout, json!({
                    "jsonrpc": "2.0",
                    "id": null,
                    "error": {"code": -32700, "message": format!("Parse error: {e}")}
                }))?;
                continue;
            }
        };

        let id = request.get("id").cloned().unwrap_or(Value::Null);
        let method = request["method"].as_str().unwrap_or("");

        let response = match method {
            "initialize" => handle_initialize(&id),
            "tools/list" => handle_tools_list(&id),
            "tools/call" => handle_tools_call(&id, &request),
            "notifications/initialized" | "notifications/cancelled" => continue,
            _ => json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": {"code": -32601, "message": format!("Unknown method: {method}")}
            }),
        };

        write_response(&stdout, response)?;
    }

    Ok(())
}

fn handle_initialize(id: &Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "protocolVersion": "2024-11-05",
            "capabilities": {
                "tools": {}
            },
            "serverInfo": {
                "name": "aide",
                "version": env!("CARGO_PKG_VERSION")
            }
        }
    })
}

fn handle_tools_list(id: &Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "tools": [
                {
                    "name": "aide_run",
                    "description": "Execute a task in an agent directory. The agent must have an Aidefile.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "agent": {
                                "type": "string",
                                "description": "Agent name (registered) or path to directory with Aidefile"
                            },
                            "task": {
                                "type": "string",
                                "description": "Task description for the agent to execute"
                            }
                        },
                        "required": ["agent", "task"]
                    }
                },
                {
                    "name": "aide_list",
                    "description": "List all registered agents with their paths and trigger status.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {}
                    }
                },
                {
                    "name": "aide_spawn",
                    "description": "Create a new agent in ~/.aide/ with an Aidefile template.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "name": {
                                "type": "string",
                                "description": "Agent name"
                            },
                            "persona": {
                                "type": "string",
                                "description": "Persona name (defaults to agent name)"
                            }
                        },
                        "required": ["name"]
                    }
                },
                {
                    "name": "aide_vault_get",
                    "description": "Get a secret value from the encrypted vault by key name.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "key": {
                                "type": "string",
                                "description": "Secret key name"
                            }
                        },
                        "required": ["key"]
                    }
                }
            ]
        }
    })
}

fn handle_tools_call(id: &Value, request: &Value) -> Value {
    let tool_name = request["params"]["name"].as_str().unwrap_or("");
    let args = &request["params"]["arguments"];

    let result = match tool_name {
        "aide_run" => tool_run(args),
        "aide_list" => tool_list(),
        "aide_spawn" => tool_spawn(args),
        "aide_vault_get" => tool_vault_get(args),
        _ => Err(anyhow::anyhow!("Unknown tool: {tool_name}")),
    };

    match result {
        Ok(content) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "content": [{"type": "text", "text": content}]
            }
        }),
        Err(e) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "content": [{"type": "text", "text": format!("Error: {e:#}")}],
                "isError": true
            }
        }),
    }
}

fn tool_run(args: &Value) -> Result<String> {
    let agent = args["agent"].as_str().ok_or_else(|| anyhow::anyhow!("missing 'agent'"))?;
    let task = args["task"].as_str().ok_or_else(|| anyhow::anyhow!("missing 'task'"))?;

    let dir = crate::registry::resolve(agent)?;
    let af = crate::aidefile::load(&dir)?;
    let result = crate::runner::run(&dir, task)?;

    Ok(format!(
        "Agent: {}\nStatus: {}\nTokens used: {}",
        af.persona.name,
        if result.success { "completed" } else { "incomplete (budget exhausted)" },
        result.tokens_used
    ))
}

fn tool_list() -> Result<String> {
    let agents = crate::registry::list()?;
    if agents.is_empty() {
        return Ok("No agents registered.".into());
    }

    let mut out = String::from("NAME | PATH | TRIGGER\n");
    for agent in agents {
        let path = std::path::PathBuf::from(shellexpand::tilde(&agent.path).as_ref());
        let trigger = if crate::aidefile::exists(&path) {
            crate::aidefile::load(&path)
                .map(|a| a.trigger.on.clone())
                .unwrap_or_else(|_| "error".into())
        } else {
            "missing".into()
        };
        out.push_str(&format!("{} | {} | {}\n", agent.name, agent.path, trigger));
    }
    Ok(out)
}

fn tool_spawn(args: &Value) -> Result<String> {
    let name = args["name"].as_str().ok_or_else(|| anyhow::anyhow!("missing 'name'"))?;
    let persona = args["persona"].as_str();

    let dir = crate::registry::aide_dir().join(name);
    if dir.exists() {
        anyhow::bail!("Agent '{name}' already exists");
    }

    std::fs::create_dir_all(&dir)?;

    let persona_name = persona.unwrap_or(name);
    let content = format!(
        "[persona]\nname = \"{persona_name}\"\n\n[budget]\ntokens = \"200k\"\n\n[trigger]\non = \"manual\"\n"
    );
    std::fs::write(dir.join("Aidefile"), content)?;
    std::fs::write(dir.join("CLAUDE.md"), format!("# {persona_name}\n"))?;
    std::fs::create_dir_all(dir.join("memory"))?;
    std::fs::create_dir_all(dir.join("skills"))?;

    crate::registry::register(name, &dir)?;

    Ok(format!("Spawned agent '{name}' at {}", dir.display()))
}

fn tool_vault_get(args: &Value) -> Result<String> {
    let key = args["key"].as_str().ok_or_else(|| anyhow::anyhow!("missing 'key'"))?;
    crate::vault::get(key)
}

fn write_response(stdout: &io::Stdout, response: Value) -> Result<()> {
    let mut out = stdout.lock();
    serde_json::to_writer(&mut out, &response)?;
    out.write_all(b"\n")?;
    out.flush()?;
    Ok(())
}
