//! Runner — spawns `claude -p` in an agent directory with budget + vault.
//!
//! This is the core primitive: one safe, budgeted, fire-and-forget execution.

use anyhow::{Context, Result};
use std::path::Path;
use std::process::{Command, Stdio};
use wait_timeout::ChildExt;

use crate::aidefile::Aidefile;
use crate::budget::BudgetTracker;
use crate::vault;

/// Result of a single task execution.
pub struct RunResult {
    pub success: bool,
    pub tokens_used: u64,
    pub output: String,
    /// Compact summary suitable for posting back to the coordinator.
    /// Bounded by `[output].max_summary_tokens` in Aidefile.
    pub summary: String,
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
    let timeout = af.budget.timeout_duration();
    let secrets = resolve_vault(&af)?;
    run_hooks(agent_dir, &af.hooks.on_spawn, &secrets)?;

    // Snapshot git state so we can report changed files in the summary.
    let git_head_before = git_head(agent_dir);

    // Wrap the user's task with summary instructions so sub-agent emits
    // a parseable <aide-summary> block. This is load-bearing for token isolation.
    let wrapped_task = wrap_task_with_summary_instructions(task, &af.output);

    let mut tracker = BudgetTracker::new(af.budget.tokens_limit(), af.budget.max_retries);
    let mut last_output = String::new();
    let mut timed_out = false;
    let mut success = false;

    while tracker.can_invoke() {
        let result = invoke_claude(
            agent_dir,
            &wrapped_task,
            &secrets,
            tracker.remaining(),
            timeout,
        )?;
        if result.timed_out {
            tracing::warn!("Task timed out");
            last_output = result.output;
            timed_out = true;
            break;
        }
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

    // Build bounded summary for coordinator consumption.
    let status = if success {
        "success"
    } else if timed_out {
        "timeout"
    } else {
        "partial"
    };
    let changed_files = git_changed_files(agent_dir, git_head_before.as_deref());
    let summary = build_summary(
        status,
        tracker.used(),
        af.budget.tokens_limit(),
        tracker.invocations(),
        &changed_files,
        &last_output,
        &af.output,
    );

    Ok(RunResult {
        success,
        tokens_used: tracker.used(),
        output: last_output,
        summary,
    })
}

/// Wrap the user-provided task with instructions requiring a summary block.
fn wrap_task_with_summary_instructions(
    task: &str,
    output_cfg: &crate::aidefile::Output,
) -> String {
    format!(
        r#"{task}

---
IMPORTANT — required summary block

When you finish, your FINAL message MUST end with a block in exactly this format:

<aide-summary>
{schema}
</aide-summary>

Rules:
- Keep the entire block under {max_tokens} tokens (roughly {max_chars} characters).
- Do not include code, logs, or long output inside the block.
- Only factual fields. No filler, no praise, no apologies.
- This block is parsed by automation and sent back to a coordinator; verbosity wastes their context.

If you cannot complete the task, still emit the block with STATUS set appropriately and NOTES describing what blocked you.
"#,
        task = task,
        schema = output_cfg.narrative_schema,
        max_tokens = output_cfg.max_summary_tokens,
        max_chars = output_cfg.max_summary_tokens as usize * 4,
    )
}

/// Extract the content between `<aide-summary>` and `</aide-summary>`, if present.
fn extract_summary_block(output: &str) -> Option<String> {
    let open = "<aide-summary>";
    let close = "</aide-summary>";
    let start = output.rfind(open)?; // last occurrence wins
    let after = start + open.len();
    let end_rel = output[after..].find(close)?;
    Some(output[after..after + end_rel].trim().to_string())
}

/// Compose the final bounded summary that goes back to the coordinator.
fn build_summary(
    status: &str,
    tokens_used: u64,
    token_limit: u64,
    retries: u32,
    changed_files: &[String],
    raw_output: &str,
    output_cfg: &crate::aidefile::Output,
) -> String {
    // Deterministic header — runner always knows these values.
    let files_line = if changed_files.is_empty() {
        "CHANGED: (none)".to_string()
    } else {
        format!("CHANGED: {}", changed_files.join(", "))
    };
    let header = format!(
        "STATUS: {status}\nTOKENS: {used}/{limit}\nRETRIES: {retries}\n{files}",
        status = status,
        used = tokens_used,
        limit = token_limit,
        retries = retries,
        files = files_line,
    );

    // Narrative from sub-agent, falls back to a minimal tail of raw output.
    let narrative = extract_summary_block(raw_output).unwrap_or_else(|| {
        // Fallback: no block found. Use a short tail so coordinator still gets *something*.
        let tail_chars = 400;
        let tail = if raw_output.chars().count() > tail_chars {
            let start = raw_output.len().saturating_sub(tail_chars);
            format!("NOTES: (no aide-summary block; tail) {}", &raw_output[start..])
        } else {
            format!("NOTES: (no aide-summary block) {}", raw_output)
        };
        tail
    });

    let combined = format!("{header}\n{narrative}");

    // Enforce total cap. 1 token ≈ 4 chars is the rough heuristic used elsewhere.
    let max_chars = (output_cfg.max_summary_tokens as usize).saturating_mul(4);
    if combined.len() > max_chars {
        let mut truncated = combined[..max_chars].to_string();
        truncated.push_str("\n...[truncated]");
        truncated
    } else {
        combined
    }
}

/// Current git HEAD SHA, or None if not a repo.
fn git_head(dir: &Path) -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(dir)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Files changed since `before_sha`, or working-tree diff if no baseline.
fn git_changed_files(dir: &Path, before_sha: Option<&str>) -> Vec<String> {
    let args: Vec<&str> = match before_sha {
        Some(sha) => vec!["diff", "--name-only", sha, "HEAD"],
        None => vec!["diff", "--name-only"],
    };
    let output = match Command::new("git").args(&args).current_dir(dir).output() {
        Ok(o) if o.status.success() => o,
        _ => return vec![],
    };
    let text = String::from_utf8_lossy(&output.stdout);
    let mut files: Vec<String> = text
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| l.to_string())
        .collect();
    // Cap to first 10 to keep summary bounded
    if files.len() > 10 {
        let extra = files.len() - 10;
        files.truncate(10);
        files.push(format!("(+{extra} more)"));
    }
    files
}

/// Spawn a single `claude -p` invocation with optional timeout.
fn invoke_claude(
    agent_dir: &Path,
    task: &str,
    secrets: &[(String, String)],
    _max_tokens: u64,
    timeout: Option<std::time::Duration>,
) -> Result<InvokeResult> {
    let mut cmd = Command::new("claude");
    cmd.arg("-p")
        .arg(task)
        .arg("--output-format")
        .arg("json")
        .current_dir(agent_dir);

    // Vault injection: secrets as env vars, NEVER in the prompt
    vault::inject(&mut cmd, secrets);

    if let Some(dur) = timeout {
        // Spawn child with piped stdout and wait with timeout
        cmd.stdout(Stdio::piped());
        let mut child = cmd.spawn().context("Failed to spawn claude")?;
        match child.wait_timeout(dur)? {
            Some(status) => {
                // Process exited within timeout
                let stdout = read_child_stdout(&mut child);
                let tokens_used = extract_token_usage(&stdout);
                Ok(InvokeResult {
                    success: status.success(),
                    tokens_used,
                    output: stdout,
                    timed_out: false,
                })
            }
            None => {
                // Timeout — kill the process
                let _ = child.kill();
                let _ = child.wait();
                Ok(InvokeResult {
                    success: false,
                    tokens_used: 0,
                    output: String::new(),
                    timed_out: true,
                })
            }
        }
    } else {
        let output = cmd.output().context("Failed to spawn claude")?;
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let tokens_used = extract_token_usage(&stdout);
        Ok(InvokeResult {
            success: output.status.success(),
            tokens_used,
            output: stdout,
            timed_out: false,
        })
    }
}

/// Read stdout from a spawned child process.
fn read_child_stdout(child: &mut std::process::Child) -> String {
    use std::io::Read;
    let mut buf = String::new();
    if let Some(ref mut stdout) = child.stdout {
        let _ = stdout.read_to_string(&mut buf);
    }
    buf
}

struct InvokeResult {
    success: bool,
    tokens_used: u64,
    output: String,
    timed_out: bool,
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
                    let _ = Command::new("git")
                        .args(["add", "memory/"])
                        .current_dir(agent_dir)
                        .output();
                    let _ = Command::new("git")
                        .args(["commit", "-m", "aide: auto-commit memory"])
                        .current_dir(agent_dir)
                        .output();
                    // Push if remote is configured
                    let has_remote = Command::new("git")
                        .args(["remote", "get-url", "origin"])
                        .current_dir(agent_dir)
                        .output()
                        .map(|o| o.status.success())
                        .unwrap_or(false);
                    if has_remote {
                        let _ = Command::new("git")
                            .args(["push"])
                            .current_dir(agent_dir)
                            .output();
                    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aidefile::Output;

    #[test]
    fn extract_summary_block_happy() {
        let out = "some preamble\n<aide-summary>\nNOTES: did the thing\nPR: none\nNEXT: none\n</aide-summary>\ntrailing";
        let block = extract_summary_block(out).unwrap();
        assert!(block.contains("NOTES: did the thing"));
        assert!(!block.contains("<aide-summary>"));
    }

    #[test]
    fn extract_summary_block_absent() {
        let out = "just random output with no block";
        assert!(extract_summary_block(out).is_none());
    }

    #[test]
    fn extract_summary_block_picks_last_occurrence() {
        let out = "<aide-summary>first</aide-summary>\n<aide-summary>second</aide-summary>";
        assert_eq!(extract_summary_block(out).unwrap(), "second");
    }

    #[test]
    fn build_summary_has_header_and_narrative() {
        let cfg = Output::default();
        let raw = "<aide-summary>\nNOTES: ok\nPR: none\nNEXT: none\n</aide-summary>";
        let s = build_summary("success", 1000, 50_000, 2, &["a.rs".into(), "b.rs".into()], raw, &cfg);
        assert!(s.contains("STATUS: success"));
        assert!(s.contains("TOKENS: 1000/50000"));
        assert!(s.contains("RETRIES: 2"));
        assert!(s.contains("CHANGED: a.rs, b.rs"));
        assert!(s.contains("NOTES: ok"));
    }

    #[test]
    fn build_summary_fallback_when_no_block() {
        let cfg = Output::default();
        let raw = "sub-agent produced verbose output but no block";
        let s = build_summary("partial", 500, 10_000, 1, &[], raw, &cfg);
        assert!(s.contains("STATUS: partial"));
        assert!(s.contains("CHANGED: (none)"));
        // Fallback path should still include something
        assert!(s.contains("no aide-summary block"));
    }

    #[test]
    fn build_summary_truncates_when_oversize() {
        let cfg = Output {
            max_summary_tokens: 50, // 200 chars
            narrative_schema: Output::default().narrative_schema,
        };
        let big_narrative = "x".repeat(10_000);
        let raw = format!("<aide-summary>\n{big_narrative}\n</aide-summary>");
        let s = build_summary("success", 1, 1, 0, &[], &raw, &cfg);
        assert!(s.len() <= 220); // 200 + truncation marker overhead
        assert!(s.contains("[truncated]"));
    }

    #[test]
    fn wrap_task_contains_schema_and_limits() {
        let cfg = Output::default();
        let wrapped = wrap_task_with_summary_instructions("do thing", &cfg);
        assert!(wrapped.contains("do thing"));
        assert!(wrapped.contains("<aide-summary>"));
        assert!(wrapped.contains("</aide-summary>"));
        assert!(wrapped.contains("NOTES:"));
        assert!(wrapped.contains("500 tokens"));
    }
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
