//! `aide dispatch` and `aide wait` — ergonomic primitives for using aide agents
//! as sub-agents from a frontier Claude session.
//!
//! Design goal: every byte printed to stdout here enters the frontier context.
//! Keep output minimal and structured.

use anyhow::{Context, Result};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::aidefile;
use crate::events::{self, Event};
use crate::registry;

/// Resolve an agent name to (local_dir, github_repo_slug).
fn resolve_agent_repo(agent: &str) -> Result<(std::path::PathBuf, String)> {
    let dir = registry::resolve(agent)?;
    let repo = github_repo_for_dir(&dir)
        .ok_or_else(|| anyhow::anyhow!("Agent '{agent}' has no github remote at {}", dir.display()))?;
    Ok((dir, repo))
}

/// Read `git remote get-url origin` and extract `owner/repo`.
fn github_repo_for_dir(dir: &Path) -> Option<String> {
    let out = Command::new("git")
        .args(["remote", "get-url", "origin"])
        .current_dir(dir)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let remote = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let repo = extract_github_repo(&remote);
    if repo.is_empty() {
        None
    } else {
        Some(repo)
    }
}

/// Extract "owner/repo" from a GitHub remote URL.
pub fn extract_github_repo(remote: &str) -> String {
    let remote = remote.trim();
    if let Some(rest) = remote.strip_prefix("https://github.com/") {
        rest.trim_end_matches(".git").to_string()
    } else if let Some(rest) = remote.strip_prefix("git@github.com:") {
        rest.trim_end_matches(".git").to_string()
    } else {
        String::new()
    }
}

/// `aide dispatch <agent> "<task>"` — create a GitHub issue labeled for the agent.
///
/// Output format (kept minimal for frontier context efficiency):
/// ```
/// dispatched: owner/repo#123
/// agent: crossmem-rs
/// budget: 50000 tokens
/// wait: aide wait https://github.com/owner/repo/issues/123
/// ```
pub fn dispatch(agent: &str, task: &str, dry_run: bool) -> Result<()> {
    let (dir, repo) = resolve_agent_repo(agent)?;
    let af = aidefile::load(&dir)?;

    // Title: first line of task, truncated
    let title = task
        .lines()
        .next()
        .unwrap_or(task)
        .chars()
        .take(80)
        .collect::<String>();

    let body = format!(
        "## Task\n\n{task}\n\n---\n_Dispatched via `aide dispatch {agent}`_\n",
    );

    if dry_run {
        println!("dry-run:");
        println!("  repo: {repo}");
        println!("  label: {agent}");
        println!("  title: {title}");
        println!("  body-bytes: {}", body.len());
        return Ok(());
    }

    let out = Command::new("gh")
        .args([
            "issue", "create",
            "--repo", &repo,
            "--label", agent,
            "--title", &title,
            "--body", &body,
        ])
        .output()
        .context("Failed to run `gh issue create`")?;

    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        anyhow::bail!("gh issue create failed: {stderr}");
    }

    // gh prints the issue URL to stdout
    let issue_url = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let number: u64 = issue_url
        .rsplit('/')
        .next()
        .and_then(|n| n.parse::<u64>().ok())
        .ok_or_else(|| anyhow::anyhow!("Could not parse issue number from {issue_url}"))?;
    let issue_ref = format!("{repo}#{number}");

    events::log(&Event {
        ts: events::now(),
        kind: "dispatched".into(),
        agent: agent.to_string(),
        issue: issue_ref.clone(),
        status: None,
        tokens: None,
    });

    // Spawn a detached background worker so dispatch returns immediately.
    // This decouples from `aide up` daemon and enables fan-out.
    spawn_background_worker(&issue_ref)?;

    println!("dispatched: {issue_ref}");
    println!("agent: {agent}");
    println!("budget: {} tokens", af.budget.tokens_limit());
    println!("wait: aide wait {issue_url}");
    Ok(())
}

/// Spawn a detached `aide run-issue <ref>` child process.
fn spawn_background_worker(issue_ref: &str) -> Result<()> {
    let exe = std::env::current_exe().context("Could not determine current exe path")?;

    // Route child output to a log file under ~/.aide/logs/ so it survives
    // parent exit without polluting the terminal the coordinator sees.
    let log_dir = registry::aide_dir().join("logs");
    std::fs::create_dir_all(&log_dir)?;
    let log_path = log_dir.join(format!(
        "dispatch-{}.log",
        issue_ref.replace('/', "_").replace('#', "-")
    ));
    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)?;
    let log_err = log_file.try_clone()?;

    Command::new(&exe)
        .args(["run-issue", issue_ref])
        .stdin(Stdio::null())
        .stdout(Stdio::from(log_file))
        .stderr(Stdio::from(log_err))
        .spawn()
        .with_context(|| format!("Failed to spawn background worker for {issue_ref}"))?;

    Ok(())
}

/// `aide run-issue <owner/repo#N>` — synchronously run one dispatched issue.
///
/// Reads the issue body + label, finds the matching agent, runs the task,
/// posts the bounded summary comment, and closes the issue on success.
pub fn run_issue(issue_ref: &str) -> Result<()> {
    let (repo, number) = parse_issue_ref(issue_ref)?;

    // Fetch issue
    let out = Command::new("gh")
        .args([
            "issue",
            "view",
            &number.to_string(),
            "--repo",
            &repo,
            "--json",
            "title,body,labels",
        ])
        .output()
        .context("Failed to run `gh issue view`")?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        anyhow::bail!("gh issue view failed: {stderr}");
    }
    let v: serde_json::Value = serde_json::from_slice(&out.stdout)?;
    let title = v["title"].as_str().unwrap_or("").to_string();
    let body = v["body"].as_str().unwrap_or("").to_string();

    // Agent name = first label (dispatch always sets exactly one label)
    let agent_name = v["labels"]
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|l| l["name"].as_str())
        .ok_or_else(|| anyhow::anyhow!("Issue has no agent label"))?
        .to_string();

    let dir = registry::resolve(&agent_name)?;
    let task = format!("{title}\n\n{body}");

    tracing::info!(
        agent = agent_name.as_str(),
        issue = number,
        "Running dispatched issue"
    );

    let issue_key = format!("{repo}#{number}");
    let task_preview = task.chars().take(200).collect::<String>();

    // Write to both JSONL (backward compat) and SQLite
    events::log(&Event {
        ts: events::now(),
        kind: "started".into(),
        agent: agent_name.clone(),
        issue: issue_key.clone(),
        status: None,
        tokens: None,
    });
    let my_pid = std::process::id();
    let run_id = crate::db::insert_run(&agent_name, &issue_key, &task_preview, Some(my_pid)).ok();

    let result = match crate::runner::run(&dir, &task) {
        Ok(r) => r,
        Err(e) => {
            events::log(&Event {
                ts: events::now(),
                kind: "failed".into(),
                agent: agent_name.clone(),
                issue: issue_key.clone(),
                status: Some(format!("error: {e}")),
                tokens: None,
            });
            if let Some(rid) = run_id {
                let _ = crate::db::finish_run(rid, false, &format!("error: {e}"), 0, 0, "");
            }
            return Err(e);
        }
    };

    // Post summary comment
    let _ = Command::new("gh")
        .args([
            "issue",
            "comment",
            &number.to_string(),
            "--repo",
            &repo,
            "--body",
            &result.summary,
        ])
        .output();

    // Close on success
    if result.success {
        let _ = Command::new("gh")
            .args(["issue", "close", &number.to_string(), "--repo", &repo])
            .output();
    }

    let status_str = extract_status(&result.summary);
    events::log(&Event {
        ts: events::now(),
        kind: "finished".into(),
        agent: agent_name,
        issue: issue_key,
        status: Some(status_str.clone()),
        tokens: Some(result.tokens_used),
    });
    if let Some(rid) = run_id {
        let _ = crate::db::finish_run(
            rid,
            result.success,
            &status_str,
            result.tokens_used,
            0,
            &result.summary,
        );

        // Populate dispatch telemetry
        let compression_ratio = if result.tokens_used > 0 {
            result.summary.len() as f64 / result.tokens_used as f64
        } else {
            0.0
        };
        let _ = crate::db::insert_telemetry(rid, result.tokens_used, compression_ratio);
    }

    Ok(())
}

/// Pull `STATUS:` line from a summary.
fn extract_status(summary: &str) -> String {
    for line in summary.lines() {
        if let Some(rest) = line.trim().strip_prefix("STATUS:") {
            return rest.trim().to_string();
        }
    }
    "unknown".into()
}

/// `aide wait <issue-url>` — block until issue closes, print final summary, exit.
///
/// Exit codes:
/// - 0: success
/// - 1: partial / failed
/// - 2: cancelled
/// - 124: timeout
pub fn wait(issue_ref: &str, timeout: Duration, poll_interval: Duration, task: Option<&str>) -> Result<i32> {
    let (repo, number) = parse_issue_ref(issue_ref)?;
    let started = Instant::now();

    loop {
        let out = Command::new("gh")
            .args([
                "issue",
                "view",
                &number.to_string(),
                "--repo",
                &repo,
                "--json",
                "state,comments",
            ])
            .output()
            .context("Failed to run `gh issue view`")?;

        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            anyhow::bail!("gh issue view failed: {stderr}");
        }

        let v: serde_json::Value = serde_json::from_slice(&out.stdout)?;
        let state = v["state"].as_str().unwrap_or("").to_uppercase();

        if state == "CLOSED" {
            // Extract last comment — should be the runner-produced summary
            let comments = v["comments"].as_array().cloned().unwrap_or_default();
            let last_comment = comments
                .last()
                .and_then(|c| c["body"].as_str())
                .unwrap_or("(no comment)")
                .to_string();

            // Record frontier-side telemetry estimates
            let issue_key = format!("{repo}#{number}");
            if let Ok(Some(run_id)) = crate::db::find_run_id_by_issue(&issue_key) {
                let dispatch_tokens = task.map(|t| t.len() as u64 / 4).unwrap_or(0);
                let wait_tokens = last_comment.len() as u64 / 4;
                let _ = crate::db::update_frontier_telemetry(run_id, dispatch_tokens, wait_tokens);
            }

            // The only thing that enters the frontier session context
            println!("{last_comment}");

            return Ok(exit_code_from_summary(&last_comment));
        }

        if started.elapsed() >= timeout {
            eprintln!("timeout after {:?}", timeout);
            return Ok(124);
        }

        std::thread::sleep(poll_interval);
    }
}

/// Parse `owner/repo#123` or `https://github.com/owner/repo/issues/123`.
pub fn parse_issue_ref(s: &str) -> Result<(String, u64)> {
    let s = s.trim();

    // Full URL form
    if let Some(rest) = s.strip_prefix("https://github.com/") {
        let parts: Vec<&str> = rest.split('/').collect();
        if parts.len() >= 4 && parts[2] == "issues" {
            let number = parts[3]
                .parse::<u64>()
                .context("Invalid issue number in URL")?;
            return Ok((format!("{}/{}", parts[0], parts[1]), number));
        }
    }

    // Short form: owner/repo#123
    if let Some((repo, num_str)) = s.split_once('#') {
        let number = num_str.parse::<u64>().context("Invalid issue number")?;
        return Ok((repo.to_string(), number));
    }

    anyhow::bail!("Unrecognized issue reference: {s}")
}

/// Determine exit code from the summary's STATUS line.
fn exit_code_from_summary(summary: &str) -> i32 {
    for line in summary.lines() {
        if let Some(rest) = line.trim().strip_prefix("STATUS:") {
            let status = rest.trim().to_lowercase();
            return match status.as_str() {
                "success" => 0,
                "cancelled" => 2,
                _ => 1,
            };
        }
    }
    1
}

/// `aide cancel <issue-ref>` — kill the background worker and close the issue.
pub fn cancel(issue_ref: &str) -> Result<()> {
    let (repo, number) = parse_issue_ref(issue_ref)?;
    let issue_key = format!("{repo}#{number}");

    let run = crate::db::get_run_by_issue(&issue_key)?;

    if let Some(ref row) = run {
        if let Some(pid) = row.worker_pid {
            tracing::info!(pid = pid, "Sending SIGTERM to worker");
            unsafe { libc::kill(pid as i32, libc::SIGTERM); }
        }
    }

    let _ = Command::new("gh")
        .args(["issue", "comment", &number.to_string(), "--repo", &repo, "--body", "STATUS: cancelled (by user)"])
        .output();
    let _ = Command::new("gh")
        .args(["issue", "close", &number.to_string(), "--repo", &repo])
        .output();

    if let Some(ref row) = run {
        let _ = crate::db::mark_cancelled(row.id);
    }

    let agent = run.as_ref().map(|r| r.agent.clone()).unwrap_or_else(|| "unknown".into());
    events::log(&Event {
        ts: events::now(),
        kind: "cancelled".into(),
        agent,
        issue: issue_key.clone(),
        status: Some("cancelled".into()),
        tokens: None,
    });

    println!("cancelled: {issue_key}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_url_form() {
        let (r, n) = parse_issue_ref("https://github.com/foo/bar/issues/42").unwrap();
        assert_eq!(r, "foo/bar");
        assert_eq!(n, 42);
    }

    #[test]
    fn parse_short_form() {
        let (r, n) = parse_issue_ref("foo/bar#42").unwrap();
        assert_eq!(r, "foo/bar");
        assert_eq!(n, 42);
    }

    #[test]
    fn parse_invalid() {
        assert!(parse_issue_ref("nonsense").is_err());
    }

    #[test]
    fn exit_code_success() {
        assert_eq!(exit_code_from_summary("STATUS: success\nTOKENS: 1/2"), 0);
    }

    #[test]
    fn exit_code_partial() {
        assert_eq!(exit_code_from_summary("STATUS: partial"), 1);
    }

    #[test]
    fn exit_code_cancelled() {
        assert_eq!(exit_code_from_summary("STATUS: cancelled"), 2);
    }

    #[test]
    fn exit_code_missing_status() {
        assert_eq!(exit_code_from_summary("no status here"), 1);
    }

    #[test]
    fn extract_github_repo_https() {
        assert_eq!(
            extract_github_repo("https://github.com/yiidtw/aide.git"),
            "yiidtw/aide"
        );
    }

    #[test]
    fn extract_github_repo_ssh() {
        assert_eq!(
            extract_github_repo("git@github.com:yiidtw/aide.git"),
            "yiidtw/aide"
        );
    }
}
