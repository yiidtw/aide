//! Daemon — polls triggers and dispatches tasks to agents.

use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{Mutex, Semaphore};

use crate::aidefile;
use crate::registry;

/// Per-agent work item produced by the tick scan phase.
enum Work {
    Issues {
        agent_name: String,
        agent_dir: PathBuf,
        af: aidefile::Aidefile,
    },
    Cron {
        agent_name: String,
        agent_dir: PathBuf,
        cron_expr: String,
    },
}

/// Start the aide daemon loop.
pub async fn start() -> Result<()> {
    let config = registry::load()?;
    let poll_secs = parse_interval(&config.daemon.poll_interval);
    let max_concurrent = config.daemon.max_concurrent;

    tracing::info!(poll_secs, max_concurrent, agents = config.agents.len(), "aide daemon started");

    // Write PID file
    let pid_path = registry::aide_dir().join("daemon.pid");
    std::fs::write(&pid_path, std::process::id().to_string())?;

    let _guard = PidGuard(pid_path.clone());

    // Global concurrency semaphore — bounds total parallel agent tasks.
    let semaphore = Arc::new(Semaphore::new(max_concurrent));

    // Per-agent locks — only ONE run per agent at a time.
    let agent_locks: Arc<Mutex<HashMap<String, Arc<Mutex<()>>>>> =
        Arc::new(Mutex::new(HashMap::new()));

    // Track last cron fire time per agent to avoid double-dispatch.
    let last_cron_fire: Arc<Mutex<HashMap<String, chrono::DateTime<chrono::Utc>>>> =
        Arc::new(Mutex::new(HashMap::new()));

    loop {
        if let Err(e) = tick(&config, &semaphore, &agent_locks, &last_cron_fire).await {
            tracing::error!("Tick error: {e:#}");
        }
        tokio::time::sleep(std::time::Duration::from_secs(poll_secs)).await;
    }
}

/// Stop the daemon by reading PID file and sending SIGTERM.
pub fn stop() -> Result<()> {
    let pid_path = registry::aide_dir().join("daemon.pid");
    if !pid_path.exists() {
        anyhow::bail!("No daemon running (no PID file)");
    }
    let pid_str = std::fs::read_to_string(&pid_path)?;
    let pid: i32 = pid_str.trim().parse()?;
    unsafe {
        libc::kill(pid, libc::SIGTERM);
    }
    std::fs::remove_file(&pid_path)?;
    tracing::info!(pid, "Daemon stopped");
    Ok(())
}

/// One tick: scan all agents for pending triggers, then dispatch concurrently.
async fn tick(
    config: &registry::Config,
    semaphore: &Arc<Semaphore>,
    agent_locks: &Arc<Mutex<HashMap<String, Arc<Mutex<()>>>>>,
    last_cron_fire: &Arc<Mutex<HashMap<String, chrono::DateTime<chrono::Utc>>>>,
) -> Result<()> {
    // ── Phase 1: Scan — collect work items (lightweight, sequential) ──
    let mut work_items: Vec<Work> = Vec::new();

    for agent in &config.agents {
        let path = PathBuf::from(shellexpand::tilde(&agent.path).as_ref());
        if !aidefile::exists(&path) {
            continue;
        }
        let af = match aidefile::load(&path) {
            Ok(af) => af,
            Err(e) => {
                tracing::warn!(agent = agent.name, "Failed to load Aidefile: {e}");
                continue;
            }
        };

        if af.trigger.is_issue() {
            work_items.push(Work::Issues {
                agent_name: agent.name.clone(),
                agent_dir: path,
                af,
            });
        } else if let Some(cron_expr) = af.trigger.cron_expr() {
            work_items.push(Work::Cron {
                agent_name: agent.name.clone(),
                agent_dir: path,
                cron_expr: cron_expr.to_string(),
            });
        }
        // manual triggers: skip (only via `aide run`)
    }

    // ── Phase 2: Dispatch — spawn concurrent tasks ──
    let mut handles = Vec::new();

    for item in work_items {
        let sem = Arc::clone(semaphore);
        let locks = Arc::clone(agent_locks);
        let cron_fire = Arc::clone(last_cron_fire);

        let handle = tokio::spawn(async move {
            match item {
                Work::Issues { agent_name, agent_dir, af } => {
                    let agent_lock = get_agent_lock(&locks, &agent_name).await;
                    let _guard = agent_lock.lock().await;
                    let _permit = sem.acquire().await.expect("semaphore closed");
                    if let Err(e) = check_github_issues(&agent_name, &agent_dir, &af).await {
                        tracing::error!(agent = agent_name, "Issue check failed: {e:#}");
                    }
                }
                Work::Cron { agent_name, agent_dir, cron_expr } => {
                    let agent_lock = get_agent_lock(&locks, &agent_name).await;
                    let _guard = agent_lock.lock().await;
                    let _permit = sem.acquire().await.expect("semaphore closed");
                    if let Err(e) = check_cron(&agent_name, &agent_dir, &cron_expr, &cron_fire).await {
                        tracing::error!(agent = agent_name, "Cron check failed: {e:#}");
                    }
                }
            }
        });
        handles.push(handle);
    }

    // Wait for all dispatched tasks to complete before the next tick.
    for handle in handles {
        if let Err(e) = handle.await {
            tracing::error!("Task join error: {e:#}");
        }
    }

    Ok(())
}

/// Get or create a per-agent lock.
async fn get_agent_lock(
    locks: &Arc<Mutex<HashMap<String, Arc<Mutex<()>>>>>,
    agent_name: &str,
) -> Arc<Mutex<()>> {
    let mut map = locks.lock().await;
    map.entry(agent_name.to_string())
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone()
}

/// Poll GitHub Issues for tasks addressed to this agent.
async fn check_github_issues(
    agent_name: &str,
    agent_dir: &PathBuf,
    _af: &aidefile::Aidefile,
) -> Result<()> {
    // Read github repo from git remote or Aidefile
    let output = std::process::Command::new("git")
        .args(["remote", "get-url", "origin"])
        .current_dir(agent_dir)
        .output()?;

    if !output.status.success() {
        return Ok(()); // No git remote, skip
    }

    let remote = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let repo = extract_github_repo(&remote);
    if repo.is_empty() {
        return Ok(());
    }

    // Use gh CLI to list open issues assigned to this agent
    let output = std::process::Command::new("gh")
        .args(["issue", "list", "--repo", &repo, "--label", agent_name, "--json", "number,title,body", "--state", "open"])
        .output()?;

    if !output.status.success() {
        return Ok(());
    }

    let issues: Vec<serde_json::Value> =
        serde_json::from_slice(&output.stdout).unwrap_or_default();

    for issue in issues {
        let number = issue["number"].as_u64().unwrap_or(0);
        let title = issue["title"].as_str().unwrap_or("");
        let body = issue["body"].as_str().unwrap_or("");
        let task = format!("{title}\n\n{body}");

        tracing::info!(agent_name, issue = number, "Dispatching issue");

        // Run the task via spawn_blocking (runner::run is synchronous)
        let dir = agent_dir.clone();
        let task_clone = task.clone();
        let run_result = tokio::task::spawn_blocking(move || {
            crate::runner::run(&dir, &task_clone)
        })
        .await?;

        match run_result {
            Ok(result) => {
                // Post the bounded summary (built by runner, capped per Aidefile [output])
                let _ = std::process::Command::new("gh")
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
                if result.success {
                    let _ = std::process::Command::new("gh")
                        .args(["issue", "close", &number.to_string(), "--repo", &repo])
                        .output();
                }
            }
            Err(e) => {
                tracing::error!(agent_name, issue = number, "Task failed: {e:#}");
            }
        }
    }
    Ok(())
}

/// Check if cron trigger should fire now.
async fn check_cron(
    agent_name: &str,
    agent_dir: &PathBuf,
    cron_expr: &str,
    last_fire: &Arc<Mutex<HashMap<String, chrono::DateTime<chrono::Utc>>>>,
) -> Result<()> {
    use std::str::FromStr;

    // cron crate uses 7-field format: sec min hour dom month dow year
    // User writes 5-field: min hour dom month dow
    // Prepend "0 " (second=0) and append " *" (year=any)
    let full_expr = format!("0 {} *", cron_expr);
    let schedule = cron::Schedule::from_str(&full_expr)
        .map_err(|e| anyhow::anyhow!("Invalid cron expression '{}': {}", cron_expr, e))?;

    let now = chrono::Utc::now();

    // Check and update last fire time atomically
    {
        let mut fire_map = last_fire.lock().await;
        let last = fire_map.get(agent_name).copied().unwrap_or(now);

        let should_fire = schedule.after(&last).take(1).any(|t| t <= now);
        if !should_fire {
            return Ok(());
        }

        fire_map.insert(agent_name.to_string(), now);
    }

    tracing::info!(agent_name, cron_expr, "Cron trigger fired");

    let task = format!("Scheduled run (cron: {cron_expr}). Check for pending work and execute.");

    let dir = agent_dir.clone();
    let task_clone = task.clone();
    let run_result = tokio::task::spawn_blocking(move || {
        crate::runner::run(&dir, &task_clone)
    })
    .await?;

    match run_result {
        Ok(result) => {
            if result.success {
                tracing::info!(agent_name, tokens = result.tokens_used, "Cron task completed");
            } else {
                tracing::warn!(
                    agent_name,
                    tokens = result.tokens_used,
                    "Cron task incomplete (budget exhausted)"
                );
            }
        }
        Err(e) => {
            tracing::error!(agent_name, "Cron task failed: {e:#}");
        }
    }
    Ok(())
}

/// Extract "owner/repo" from a GitHub remote URL.
fn extract_github_repo(remote: &str) -> String {
    let remote = remote.trim();
    if let Some(rest) = remote.strip_prefix("https://github.com/") {
        rest.trim_end_matches(".git").to_string()
    } else if let Some(rest) = remote.strip_prefix("git@github.com:") {
        rest.trim_end_matches(".git").to_string()
    } else {
        String::new()
    }
}

fn parse_interval(s: &str) -> u64 {
    let s = s.trim().to_lowercase();
    if let Some(n) = s.strip_suffix('s') {
        n.parse().unwrap_or(60)
    } else if let Some(n) = s.strip_suffix('m') {
        n.parse::<u64>().unwrap_or(1) * 60
    } else {
        s.parse().unwrap_or(60)
    }
}

/// Clean up PID file on drop.
struct PidGuard(PathBuf);

impl Drop for PidGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_github_repo() {
        assert_eq!(
            extract_github_repo("https://github.com/yiidtw/aide.git"),
            "yiidtw/aide"
        );
        assert_eq!(
            extract_github_repo("git@github.com:yiidtw/aide.git"),
            "yiidtw/aide"
        );
        assert_eq!(extract_github_repo("not-github"), "");
    }

    #[test]
    fn test_parse_interval() {
        assert_eq!(parse_interval("60s"), 60);
        assert_eq!(parse_interval("5m"), 300);
        assert_eq!(parse_interval("120"), 120);
    }

    #[test]
    fn test_cron_parse_5field() {
        use std::str::FromStr;
        // User writes 5-field, we prepend "0 " and append " *"
        let user_expr = "0 8 * * *"; // daily at 8:00
        let full = format!("0 {} *", user_expr);
        let schedule = cron::Schedule::from_str(&full);
        assert!(schedule.is_ok(), "Should parse 5-field cron: {:?}", schedule.err());
    }

    #[test]
    fn test_cron_schedule_fires() {
        use std::str::FromStr;
        let schedule = cron::Schedule::from_str("* * * * * * *").unwrap();
        let now = chrono::Utc::now();
        let past = now - chrono::Duration::seconds(5);
        let fired = schedule.after(&past).take(1).any(|t| t <= now);
        assert!(fired, "Every-second schedule should fire");
    }
}
