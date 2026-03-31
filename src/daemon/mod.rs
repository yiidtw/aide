use anyhow::{Context, Result};
use chrono::{Datelike, Timelike};
use std::path::{Path, PathBuf};
use tokio::time::Duration;
use tracing::{error, info, warn};
use wait_timeout::ChildExt;

/// Return the path to `~/.aide/daemon.pid`.
fn pid_file_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".aide").join("daemon.pid")
}

/// Write the current process PID to the PID file.
fn write_pid_file() -> Result<()> {
    let path = pid_file_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, std::process::id().to_string())?;
    Ok(())
}

/// Remove the PID file if it exists.
fn remove_pid_file() {
    let _ = std::fs::remove_file(pid_file_path());
}

/// Stop the running daemon. Returns Ok(true) if a daemon was stopped,
/// Ok(false) if no daemon was running.
pub fn stop_daemon() -> Result<bool> {
    let path = pid_file_path();

    let pid_str = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(_) => {
            return Ok(false);
        }
    };

    let pid: u32 = match pid_str.trim().parse() {
        Ok(p) => p,
        Err(_) => {
            // Stale / corrupt PID file — clean up
            let _ = std::fs::remove_file(&path);
            return Ok(false);
        }
    };

    // Check if process is alive (signal 0)
    let alive = std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if !alive {
        // Stale PID file
        let _ = std::fs::remove_file(&path);
        return Ok(false);
    }

    // Send SIGTERM
    let sent = std::process::Command::new("kill")
        .args([&pid.to_string()])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if !sent {
        anyhow::bail!("failed to send SIGTERM to pid {}", pid);
    }

    // Wait up to 10 seconds for process to exit
    for _ in 0..100 {
        let still_alive = std::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !still_alive {
            let _ = std::fs::remove_file(&path);
            return Ok(true);
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    // Force kill
    let _ = std::process::Command::new("kill")
        .args(["-9", &pid.to_string()])
        .status();
    let _ = std::fs::remove_file(&path);
    Ok(true)
}

use crate::agents::agentfile::AgentfileSpec;
use crate::agents::instance::InstanceManager;
use crate::config::AideConfig;
use crate::dashboard;
use crate::email::GmailPoller;

const DASHBOARD_PORT: u16 = 3939;

pub struct Daemon {
    config: AideConfig,
    dash_enabled: bool,
}


impl Daemon {
    pub fn new(config: AideConfig) -> Self {
        Self {
            config,
            dash_enabled: true,
        }
    }

    pub fn with_dash(mut self, enabled: bool) -> Self {
        self.dash_enabled = enabled;
        self
    }

    pub async fn run(&self) -> Result<()> {
        // Write PID file so `aide down` can find us
        write_pid_file().context("failed to write daemon PID file")?;

        info!(
            name = %self.config.aide.name,
            machines = self.config.machines.len(),
            agents = self.config.agents.len(),
            "aide daemon starting"
        );

        // Log dispatch rules
        for (task, rule) in &self.config.dispatch {
            info!(task = %task, on = %rule.on, "dispatch rule loaded");
        }

        // Log agents
        for (name, agent) in &self.config.agents {
            info!(name = %name, email = %agent.email, role = %agent.role, "agent registered");
        }

        // Start Gmail poller if credentials available
        self.start_gmail_poller();

        // Start dashboard
        if self.dash_enabled {
            dashboard::spawn_dashboard(self.config.aide.data_dir.clone(), DASHBOARD_PORT);
            info!(port = DASHBOARD_PORT, "dashboard at http://localhost:{}", DASHBOARD_PORT);
        }

        // Start cron ticker
        self.start_cron_ticker();

        // Start daily cognition commit ticker
        self.start_daily_commit_ticker();

        // Start Telegram bots for instances that declare [expose.telegram]
        self.start_telegram_bots();

        // Start GitHub Issues pollers for instances with github_repo
        self.start_github_issues_poller();

        info!("aide daemon ready, waiting for signals");

        // Wait for SIGINT or SIGTERM
        #[cfg(unix)]
        {
            use tokio::signal::unix::{signal, SignalKind};
            let mut sigint = signal(SignalKind::interrupt())?;
            let mut sigterm = signal(SignalKind::terminate())?;
            tokio::select! {
                _ = sigint.recv() => {
                    info!("received SIGINT, shutting down");
                }
                _ = sigterm.recv() => {
                    info!("received SIGTERM, shutting down");
                }
            }
        }

        #[cfg(not(unix))]
        {
            match signal::ctrl_c().await {
                Ok(()) => {
                    info!("received SIGINT, shutting down");
                }
                Err(err) => {
                    warn!("failed to listen for shutdown signal: {}", err);
                }
            }
        }

        remove_pid_file();
        info!("aide daemon stopped");
        Ok(())
    }

    fn start_gmail_poller(&self) {
        // Try loading credentials from vault env file or environment
        let creds = self.load_gmail_creds();

        let Some((client_id, client_secret, refresh_token)) = creds else {
            warn!("Gmail credentials not found, email poller disabled");
            warn!("Set AIDE_GOOGLE_CLIENT_ID, AIDE_GOOGLE_CLIENT_SECRET, AIDE_GMAIL_REFRESH_TOKEN");
            return;
        };

        let poll_interval = Duration::from_secs(60);
        info!(interval_secs = 60, "starting Gmail poller");

        tokio::spawn(async move {
            let mut poller = GmailPoller::new(
                client_id,
                client_secret,
                refresh_token,
                poll_interval,
            );

            if let Err(e) = poller.run_poll_loop().await {
                error!(error = %e, "Gmail poller exited with error");
            }
        });
    }

    fn load_gmail_creds(&self) -> Option<(String, String, String)> {
        // Try environment variables first
        let client_id = std::env::var("AIDE_GOOGLE_CLIENT_ID").ok()?;
        let client_secret = std::env::var("AIDE_GOOGLE_CLIENT_SECRET").ok()?;
        let refresh_token = std::env::var("AIDE_GMAIL_REFRESH_TOKEN").ok()?;
        Some((client_id, client_secret, refresh_token))
    }

    fn start_telegram_bots(&self) {
        let data_dir = self.config.aide.data_dir.clone();
        let mgr = InstanceManager::new(&data_dir);
        let instances = match mgr.list() {
            Ok(v) => v,
            Err(_) => return,
        };

        let vault_env = load_vault_env().unwrap_or_default();

        for inst in &instances {
            let inst_dir = mgr.base_dir().join(&inst.name);
            let spec = match AgentfileSpec::load(&inst_dir) {
                Ok(s) => s,
                Err(_) => continue,
            };

            if let Some(expose) = &spec.expose {
                if let Some(tg) = &expose.telegram {
                    // Find token from vault env
                    let token = vault_env
                        .iter()
                        .find(|(k, _)| k == &tg.token_env)
                        .map(|(_, v)| v.clone());

                    if let Some(token) = token {
                        info!(instance = %inst.name, "starting telegram bot");
                        crate::expose::telegram::spawn_telegram_bot(
                            data_dir.clone(),
                            inst.name.clone(),
                            token,
                        );
                    } else {
                        warn!(
                            instance = %inst.name,
                            env = %tg.token_env,
                            "telegram token not found in vault"
                        );
                    }
                }
            }
        }
    }

    fn start_github_issues_poller(&self) {
        let data_dir = self.config.aide.data_dir.clone();
        crate::expose::github::start_github_issues_ticker(data_dir);
    }

    fn start_cron_ticker(&self) {
        let data_dir = self.config.aide.data_dir.clone();
        info!("starting cron ticker (60s interval)");

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60));
            loop {
                interval.tick().await;
                if let Err(e) = cron_tick(&data_dir) {
                    error!(error = %e, "cron tick failed");
                }
            }
        });
    }

    fn start_daily_commit_ticker(&self) {
        let data_dir = self.config.aide.data_dir.clone();
        let commit_hour = self.config.aide.daily_commit_hour;
        info!(hour = commit_hour, "starting daily cognition commit ticker");

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60));
            loop {
                interval.tick().await;
                let now = chrono::Local::now();
                if now.hour() == commit_hour && now.minute() == 0 {
                    let results = crate::agents::commit::daily_commit_all(&data_dir);
                    if results.is_empty() {
                        info!("daily cognition commit: all clean");
                    } else {
                        for (name, summary) in &results {
                            info!(instance = %name, "daily cognition commit:\n{}", summary);
                        }
                    }
                }
            }
        });
    }
}

// ─── Cron ticker logic ───

/// Run a single cron tick: check all instances for due cron entries and execute them.
/// Enforces max_timeout and max_retry from [limits] in Agentfile.toml.
/// On failure, opens a GitHub issue on the instance's repo if configured.
fn cron_tick(data_dir: &str) -> Result<()> {
    let mgr = InstanceManager::new(data_dir);
    let instances = mgr.list()?;

    if instances.is_empty() {
        return Ok(());
    }

    let vault_env = load_vault_env().unwrap_or_default();
    let now = chrono::Local::now();

    for inst in &instances {
        if inst.cron_count == 0 {
            continue;
        }

        let cron_entries = match mgr.cron_list(&inst.name) {
            Ok(entries) => entries,
            Err(e) => {
                warn!(instance = %inst.name, error = %e, "failed to load cron entries");
                continue;
            }
        };

        let inst_dir = mgr.base_dir().join(&inst.name);

        // Load limits from Agentfile.toml
        let (timeout_secs, max_retry) = match AgentfileSpec::load(&inst_dir) {
            Ok(spec) => {
                let t = spec.limits.as_ref().map(|l| l.max_timeout).unwrap_or(300);
                let r = spec.limits.as_ref().map(|l| l.max_retry).unwrap_or(0);
                (t, r)
            }
            Err(_) => (300, 0),
        };

        // Load github_repo for failure alerting
        let github_repo = mgr.get(&inst.name)
            .ok()
            .flatten()
            .and_then(|m| m.github_repo);

        for entry in &cron_entries {
            if !cron_matches_now(&entry.schedule, &now) {
                continue;
            }

            // Prevent double-fire: skip if last_run was less than 59 seconds ago
            if let Some(last_run) = entry.last_run {
                let elapsed = chrono::Utc::now() - last_run;
                if elapsed.num_seconds() < 59 {
                    continue;
                }
            }

            info!(
                instance = %inst.name,
                skill = %entry.skill,
                schedule = %entry.schedule,
                "cron triggered"
            );

            // Log before execution
            if let Err(e) = mgr.append_log(&inst.name, &format!("cron: {} triggered", entry.skill))
            {
                warn!(error = %e, "failed to append cron log");
            }

            // Execute with retry
            let mut last_result = None;
            let attempts = max_retry + 1; // 0 retries = 1 attempt
            for attempt in 1..=attempts {
                if attempt > 1 {
                    let backoff = std::time::Duration::from_secs(2u64.pow(attempt - 1));
                    info!(
                        instance = %inst.name,
                        skill = %entry.skill,
                        attempt,
                        backoff_secs = backoff.as_secs(),
                        "retrying after backoff"
                    );
                    let _ = mgr.append_log(
                        &inst.name,
                        &format!("cron-retry: {} attempt {}/{}", entry.skill, attempt, attempts),
                    );
                    std::thread::sleep(backoff);
                }

                let result = exec_skill_or_aide_skill(&inst_dir, &entry.skill, "", &vault_env, timeout_secs);

                match &result {
                    Ok((exit_code, _, _)) if *exit_code == 0 => {
                        last_result = Some(result);
                        break; // success, no more retries
                    }
                    _ => {
                        last_result = Some(result);
                        // continue to next attempt if retries remain
                    }
                }
            }

            // Process final result
            let result = last_result.unwrap();
            match &result {
                Ok((exit_code, stdout, stderr)) => {
                    let status = if *exit_code == 0 { "ok" } else { "FAILED" };
                    info!(
                        instance = %inst.name,
                        skill = %entry.skill,
                        exit_code,
                        "cron skill finished"
                    );
                    let log_msg = format!(
                        "cron-result: {} → {} (exit {})",
                        entry.skill, status, exit_code
                    );
                    if let Err(e) = mgr.append_log(&inst.name, &log_msg) {
                        warn!(error = %e, "failed to append cron result log");
                    }
                    // Log truncated stdout/stderr for debugging
                    if !stdout.trim().is_empty() {
                        let truncated: String = stdout.chars().take(2000).collect();
                        let _ = mgr.append_log(
                            &inst.name,
                            &format!("cron-stdout: {}: {}", entry.skill, truncated.trim()),
                        );
                    }
                    if !stderr.trim().is_empty() {
                        let truncated: String = stderr.chars().take(2000).collect();
                        let _ = mgr.append_log(
                            &inst.name,
                            &format!("cron-stderr: {}: {}", entry.skill, truncated.trim()),
                        );
                    }
                    // Alert on non-zero exit after all retries exhausted
                    if *exit_code != 0 {
                        alert_cron_failure(
                            &inst.name, &entry.skill, github_repo.as_deref(),
                            &format!("exit code {} after {} attempt(s)\n\nstderr:\n```\n{}\n```", exit_code, attempts, stderr.chars().take(1000).collect::<String>()),
                        );
                    }
                }
                Err(e) => {
                    error!(
                        instance = %inst.name,
                        skill = %entry.skill,
                        error = %e,
                        "cron skill execution failed"
                    );
                    let log_msg = format!("cron-result: {} → error: {}", entry.skill, e);
                    let _ = mgr.append_log(&inst.name, &log_msg);
                    // Alert on execution failure
                    alert_cron_failure(
                        &inst.name, &entry.skill, github_repo.as_deref(),
                        &format!("{}", e),
                    );
                }
            }

            // Update last_run regardless of success/failure
            if let Err(e) = mgr.cron_update_last_run(&inst.name, &entry.skill) {
                warn!(
                    instance = %inst.name,
                    skill = %entry.skill,
                    error = %e,
                    "failed to update last_run"
                );
            }
        }
    }

    Ok(())
}

/// Alert on cron skill failure by opening a GitHub issue on the instance's repo.
/// Silent no-op if no github_repo is configured or gh CLI is unavailable.
fn alert_cron_failure(instance: &str, skill: &str, github_repo: Option<&str>, details: &str) {
    let Some(repo) = github_repo else {
        warn!(instance, skill, "cron failure alert skipped: no github_repo configured");
        return;
    };

    let title = format!("[aide] cron failure: {} on {}", skill, instance);
    let body = format!(
        "Automated alert from aide daemon.\n\n\
         - **Instance**: {}\n\
         - **Skill**: {}\n\
         - **Time**: {}\n\n\
         ## Details\n\n{}\n\n\
         ---\n_This issue was opened automatically by `aide up` cron executor._",
        instance, skill, chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"), details
    );

    let result = std::process::Command::new("gh")
        .args(["issue", "create", "--repo", repo, "--title", &title, "--body", &body, "--label", "aide-alert"])
        .output();

    match result {
        Ok(output) if output.status.success() => {
            let url = String::from_utf8_lossy(&output.stdout);
            info!(instance, skill, url = url.trim(), "opened failure alert issue");
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!(instance, skill, error = stderr.trim().to_string(), "failed to open alert issue");
        }
        Err(e) => {
            warn!(instance, skill, error = %e, "gh CLI not available for alert");
        }
    }
}

/// Check if a 5-field cron expression matches the given local time.
///
/// Supports: `*` (any), `N` (exact), `*/N` (every N), `N-M` (range), comma-separated lists.
/// Fields: minute hour day_of_month month day_of_week
fn cron_matches_now(schedule: &str, now: &chrono::DateTime<chrono::Local>) -> bool {
    let fields: Vec<&str> = schedule.split_whitespace().collect();
    if fields.len() != 5 {
        warn!(schedule, "invalid cron expression: expected 5 fields");
        return false;
    }

    let checks = [
        (fields[0], now.minute()),
        (fields[1], now.hour()),
        (fields[2], now.day()),
        (fields[3], now.month()),
        (fields[4], now.weekday().num_days_from_sunday()),
    ];

    checks
        .iter()
        .all(|(field, value)| cron_field_matches(field, *value))
}

/// Check if a single cron field matches a given value.
fn cron_field_matches(field: &str, value: u32) -> bool {
    // Handle comma-separated values: "1,15,30"
    if field.contains(',') {
        return field.split(',').any(|part| cron_field_matches(part, value));
    }

    // Wildcard
    if field == "*" {
        return true;
    }

    // Step: */N
    if let Some(step_str) = field.strip_prefix("*/") {
        if let Ok(step) = step_str.parse::<u32>() {
            return step > 0 && value % step == 0;
        }
        return false;
    }

    // Range: N-M
    if field.contains('-') {
        let parts: Vec<&str> = field.splitn(2, '-').collect();
        if parts.len() == 2 {
            if let (Ok(start), Ok(end)) = (parts[0].parse::<u32>(), parts[1].parse::<u32>()) {
                return value >= start && value <= end;
            }
        }
        return false;
    }

    // Exact match
    if let Ok(n) = field.parse::<u32>() {
        return value == n;
    }

    false
}

/// Find a skill script, trying occupation/skills/ first, then skills/, then aide-skill.
fn find_skill_script(inst_dir: &Path, skill_name: &str) -> Option<PathBuf> {
    // 1. Local instance skills
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

/// Execute a skill, trying local scripts first, then falling back to aide-skill CLI.
fn exec_skill_or_aide_skill(
    inst_dir: &Path,
    skill_name: &str,
    args: &str,
    env: &[(String, String)],
    timeout_secs: u64,
) -> Result<(i32, String, String)> {
    // Try local script first
    if let Some(_script) = find_skill_script(inst_dir, skill_name) {
        return exec_cron_skill(inst_dir, skill_name, env, timeout_secs);
    }
    // Fallback: aide-skill CLI
    info!(skill = skill_name, "local script not found, trying aide-skill");
    let mut cmd = std::process::Command::new("aide-skill");
    cmd.arg(skill_name);
    if !args.is_empty() {
        for a in args.split_whitespace() {
            cmd.arg(a);
        }
    }
    cmd.current_dir(inst_dir);
    cmd.env("AIDE_INSTANCE_DIR", inst_dir);
    for (k, v) in env {
        cmd.env(k, v);
    }
    let mut child = cmd
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .with_context(|| format!("aide-skill {} not found", skill_name))?;
    let timeout = std::time::Duration::from_secs(timeout_secs);
    match child.wait_timeout(timeout) {
        Ok(Some(status)) => {
            let stdout = child.stdout.take().map(|mut s| {
                let mut buf = String::new();
                std::io::Read::read_to_string(&mut s, &mut buf).ok();
                buf
            }).unwrap_or_default();
            let stderr = child.stderr.take().map(|mut s| {
                let mut buf = String::new();
                std::io::Read::read_to_string(&mut s, &mut buf).ok();
                buf
            }).unwrap_or_default();
            Ok((status.code().unwrap_or(-1), stdout, stderr))
        }
        Ok(None) => {
            let _ = child.kill();
            let _ = child.wait();
            anyhow::bail!("aide-skill '{}' timed out after {}s", skill_name, timeout_secs)
        }
        Err(e) => {
            let _ = child.kill();
            let _ = child.wait();
            Err(e).context(format!("failed waiting on aide-skill {}", skill_name))
        }
    }
}

/// Execute a skill script for a cron entry, with timeout enforcement.
fn exec_cron_skill(
    inst_dir: &Path,
    skill_name: &str,
    env: &[(String, String)],
    timeout_secs: u64,
) -> Result<(i32, String, String)> {
    // Try occupation/skills/ first, then skills/ for backward compat
    let script = find_skill_script(inst_dir, skill_name);

    let script = match script {
        Some(s) => s,
        None => anyhow::bail!("skill script not found: {}/occupation/skills/{}.{{ts,sh}}", inst_dir.display(), skill_name),
    };

    let ext = script.extension().and_then(|e| e.to_str()).unwrap_or("sh");

    let mut cmd = if ext == "ts" {
        let bun = crate::find_or_install_bun()?;
        let mut c = std::process::Command::new(bun);
        c.arg("run");
        c.arg(&script);
        c
    } else {
        let mut c = std::process::Command::new("bash");
        c.arg(&script);
        c
    };

    cmd.current_dir(inst_dir);
    cmd.env("AIDE_INSTANCE_DIR", inst_dir);

    for (k, v) in env {
        cmd.env(k, v);
    }

    // Spawn as child process so we can enforce timeout
    let mut child = cmd
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .with_context(|| format!("failed to spawn skill script: {}", script.display()))?;

    let timeout = std::time::Duration::from_secs(timeout_secs);
    match child.wait_timeout(timeout) {
        Ok(Some(status)) => {
            // Process exited within timeout
            let stdout = child.stdout.take().map(|mut s| {
                let mut buf = String::new();
                std::io::Read::read_to_string(&mut s, &mut buf).ok();
                buf
            }).unwrap_or_default();
            let stderr = child.stderr.take().map(|mut s| {
                let mut buf = String::new();
                std::io::Read::read_to_string(&mut s, &mut buf).ok();
                buf
            }).unwrap_or_default();
            Ok((status.code().unwrap_or(-1), stdout, stderr))
        }
        Ok(None) => {
            // Timeout — kill the process
            warn!(skill = skill_name, timeout_secs, "skill execution timed out, killing process");
            let _ = child.kill();
            let _ = child.wait(); // reap zombie
            anyhow::bail!("skill '{}' timed out after {}s", skill_name, timeout_secs)
        }
        Err(e) => {
            let _ = child.kill();
            let _ = child.wait();
            Err(e).context(format!("failed waiting on skill script: {}", script.display()))
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cron_field_wildcard() {
        assert!(cron_field_matches("*", 0));
        assert!(cron_field_matches("*", 59));
    }

    #[test]
    fn test_cron_field_exact() {
        assert!(cron_field_matches("30", 30));
        assert!(!cron_field_matches("30", 15));
    }

    #[test]
    fn test_cron_field_step() {
        assert!(cron_field_matches("*/15", 0));
        assert!(cron_field_matches("*/15", 15));
        assert!(cron_field_matches("*/15", 30));
        assert!(!cron_field_matches("*/15", 7));
    }

    #[test]
    fn test_cron_field_range() {
        assert!(cron_field_matches("1-5", 1));
        assert!(cron_field_matches("1-5", 3));
        assert!(cron_field_matches("1-5", 5));
        assert!(!cron_field_matches("1-5", 0));
        assert!(!cron_field_matches("1-5", 6));
    }

    #[test]
    fn test_cron_field_comma() {
        assert!(cron_field_matches("0,15,30,45", 0));
        assert!(cron_field_matches("0,15,30,45", 15));
        assert!(!cron_field_matches("0,15,30,45", 10));
    }

    #[test]
    fn test_cron_matches_every_minute() {
        let now = chrono::Local::now();
        assert!(cron_matches_now("* * * * *", &now));
    }

    #[test]
    fn test_cron_matches_impossible() {
        let now = chrono::Local::now();
        // Month 13 doesn't exist
        assert!(!cron_matches_now("* * * 13 *", &now));
    }
}
