use anyhow::{Context, Result};
use chrono::{Datelike, Timelike};
use std::path::{Path, PathBuf};
use tokio::signal;
use tokio::time::Duration;
use tracing::{error, info, warn};

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

        // Start Telegram bots for instances that declare [expose.telegram]
        self.start_telegram_bots();

        // Start GitHub Issues pollers for instances with github_repo
        self.start_github_issues_poller();

        info!("aide daemon ready, waiting for signals");

        // Wait for shutdown signal
        match signal::ctrl_c().await {
            Ok(()) => {
                info!("received SIGINT, shutting down");
            }
            Err(err) => {
                warn!("failed to listen for shutdown signal: {}", err);
            }
        }

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
}

// ─── Cron ticker logic ───

/// Run a single cron tick: check all instances for due cron entries and execute them.
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

            // Execute the skill
            let result = exec_cron_skill(&inst_dir, &entry.skill, &vault_env);

            match &result {
                Ok((exit_code, stdout, stderr)) => {
                    let status = if *exit_code == 0 { "ok" } else { "fail" };
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
                        let truncated: String = stdout.chars().take(500).collect();
                        let _ = mgr.append_log(
                            &inst.name,
                            &format!("cron-stdout: {}: {}", entry.skill, truncated.trim()),
                        );
                    }
                    if !stderr.trim().is_empty() {
                        let truncated: String = stderr.chars().take(500).collect();
                        let _ = mgr.append_log(
                            &inst.name,
                            &format!("cron-stderr: {}: {}", entry.skill, truncated.trim()),
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

/// Execute a skill script for a cron entry.
fn exec_cron_skill(
    inst_dir: &Path,
    skill_name: &str,
    env: &[(String, String)],
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

    let output = cmd
        .output()
        .with_context(|| format!("failed to execute skill script: {}", script.display()))?;

    Ok((
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    ))
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
