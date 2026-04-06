//! Runner — spawns `claude -p` in an agent directory with budget + vault.
//!
//! This is the core primitive: one safe, budgeted, fire-and-forget execution.

use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;

use crate::aidefile::Aidefile;
use crate::budget::BudgetTracker;
use crate::vault;

/// Result of a single task execution.
pub struct RunResult {
    pub success: bool,
    pub tokens_used: u64,
    pub output: String,
}

/// Execute a task in an agent directory.
///
/// 1. Parse Aidefile
/// 2. Decrypt vault secrets
/// 3. Run on_spawn hooks
/// 4. Loop: claude -p until done or budget exhausted
/// 5. Run on_complete hooks
pub fn run(agent_dir: &Path, task: &str) -> Result<RunResult> {
    let af = crate::aidefile::load(agent_dir)?;
    let secrets = resolve_vault(&af)?;
    run_hooks(agent_dir, &af.hooks.on_spawn, &secrets)?;

    let mut tracker = BudgetTracker::new(af.budget.tokens_limit(), af.budget.max_retries);
    let mut last_output = String::new();
    let mut success = false;

    while tracker.can_invoke() {
        let result = invoke_claude(agent_dir, task, &secrets, tracker.remaining())?;
        tracker.record(result.tokens_used);
        last_output = result.output;
        if result.success {
            success = true;
            break;
        }
        tracing::info!(
            invocation = tracker.invocations(),
            used = tracker.used(),
            remaining = tracker.remaining(),
            "Task incomplete, retrying"
        );
    }

    run_hooks(agent_dir, &af.hooks.on_complete, &secrets)?;

    // Check memory compaction
    check_memory_compact(agent_dir, &af)?;

    Ok(RunResult {
        success,
        tokens_used: tracker.used(),
        output: last_output,
    })
}

/// Spawn a single `claude -p` invocation.
fn invoke_claude(
    agent_dir: &Path,
    task: &str,
    secrets: &[(String, String)],
    _max_tokens: u64,
) -> Result<InvokeResult> {
    let mut cmd = Command::new("claude");
    cmd.arg("-p")
        .arg(task)
        .arg("--output-format")
        .arg("json")
        .current_dir(agent_dir);

    // Vault injection: secrets as env vars, NEVER in the prompt
    vault::inject(&mut cmd, secrets);

    let output = cmd.output().context("Failed to spawn claude")?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let tokens_used = extract_token_usage(&stdout);

    Ok(InvokeResult {
        success: output.status.success(),
        tokens_used,
        output: stdout,
    })
}

struct InvokeResult {
    success: bool,
    tokens_used: u64,
    output: String,
}

/// Extract token usage from claude JSON output.
fn extract_token_usage(json_output: &str) -> u64 {
    // claude -p --output-format json returns {"result": "...", "usage": {"input": N, "output": N}}
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(json_output) {
        let input = v["usage"]["input_tokens"].as_u64().unwrap_or(0);
        let output = v["usage"]["output_tokens"].as_u64().unwrap_or(0);
        input + output
    } else {
        0
    }
}

/// Resolve vault secrets from Aidefile config.
fn resolve_vault(af: &Aidefile) -> Result<Vec<(String, String)>> {
    if af.vault.keys.is_empty() {
        return Ok(vec![]);
    }
    vault::decrypt_keys(
        &vault::default_vault_path(),
        &vault::default_key_path(),
        &af.vault.keys,
    )
}

/// Run a list of hook scripts in the agent directory.
fn run_hooks(agent_dir: &Path, hooks: &[String], secrets: &[(String, String)]) -> Result<()> {
    for hook in hooks {
        match hook.as_str() {
            "inject-vault" => {
                // Built-in: vault injection is handled by the runner itself.
                tracing::debug!("inject-vault: handled by runner");
            }
            "commit-memory" => {
                let memory_dir = agent_dir.join("memory");
                if memory_dir.exists() {
                    let mut cmd = Command::new("git");
                    cmd.args(["add", "memory/"])
                        .current_dir(agent_dir);
                    let _ = cmd.output();
                    let mut cmd = Command::new("git");
                    cmd.args(["commit", "-m", "aide: auto-commit memory"])
                        .current_dir(agent_dir);
                    let _ = cmd.output();
                }
            }
            _ => {
                // Custom hook: run as shell script
                let hook_path = agent_dir.join(hook);
                if hook_path.exists() {
                    let mut cmd = Command::new("sh");
                    cmd.arg(&hook_path).current_dir(agent_dir);
                    vault::inject(&mut cmd, secrets);
                    cmd.output()
                        .with_context(|| format!("Hook failed: {hook}"))?;
                } else {
                    tracing::warn!("Hook script not found: {hook}");
                }
            }
        }
    }
    Ok(())
}

/// Check if memory needs compaction.
fn check_memory_compact(agent_dir: &Path, af: &Aidefile) -> Result<()> {
    let memory_dir = agent_dir.join("memory");
    if !memory_dir.exists() {
        return Ok(());
    }

    let total_bytes: u64 = walkdir(memory_dir.as_path());
    // Rough estimate: 1 token ≈ 4 bytes
    let estimated_tokens = total_bytes / 4;
    let threshold = af.memory.compact_threshold();

    if estimated_tokens > threshold {
        tracing::info!(
            estimated_tokens,
            threshold,
            "Memory exceeds compact threshold, triggering compaction"
        );
        let mut cmd = Command::new("claude");
        cmd.arg("-p")
            .arg("Compact your memory. Summarize and deduplicate files in memory/. Remove redundant entries.")
            .current_dir(agent_dir);
        let _ = cmd.output();
    }

    Ok(())
}

/// Sum file sizes in a directory (non-recursive for simplicity).
fn walkdir(dir: &Path) -> u64 {
    let mut total = 0u64;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            if let Ok(meta) = entry.metadata() {
                if meta.is_file() {
                    total += meta.len();
                }
            }
        }
    }
    total
}
