//! Daemon — polls triggers and dispatches tasks to agents.

use anyhow::Result;
use std::path::PathBuf;

use crate::aidefile;
use crate::registry;

/// Start the aide daemon loop.
pub async fn start() -> Result<()> {
    let config = registry::load()?;
    let poll_secs = parse_interval(&config.daemon.poll_interval);

    tracing::info!(poll_secs, agents = config.agents.len(), "aide daemon started");

    // Write PID file
    let pid_path = registry::aide_dir().join("daemon.pid");
    std::fs::write(&pid_path, std::process::id().to_string())?;

    let _guard = PidGuard(pid_path.clone());

    loop {
        if let Err(e) = tick(&config).await {
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

/// One tick: check all agents for pending triggers.
async fn tick(config: &registry::Config) -> Result<()> {
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
            check_github_issues(&agent.name, &path, &af).await?;
        } else if let Some(cron_expr) = af.trigger.cron_expr() {
            check_cron(&agent.name, &path, cron_expr)?;
        }
        // manual triggers: skip (only via `aide run`)
    }
    Ok(())
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

        // Run the task
        match crate::runner::run(agent_dir, &task) {
            Ok(result) => {
                // Comment result on issue and close
                let comment = if result.success {
                    format!("Task completed. Tokens used: {}", result.tokens_used)
                } else {
                    format!(
                        "Task incomplete (budget exhausted). Tokens used: {}",
                        result.tokens_used
                    )
                };
                let _ = std::process::Command::new("gh")
                    .args(["issue", "comment", &number.to_string(), "--repo", &repo, "--body", &comment])
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
fn check_cron(_agent_name: &str, _agent_dir: &PathBuf, _cron_expr: &str) -> Result<()> {
    // TODO: implement cron schedule matching
    // For now, rely on system crontab
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
}
