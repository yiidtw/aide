mod agents;
mod company;
mod config;
mod daemon;
mod dashboard;
mod dispatch;
mod email;
mod expose;
mod hub;
mod mcp;
mod sync;
mod top;
mod vault;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};
use tracing_subscriber::EnvFilter;

use agents::agentfile::AgentfileSpec;
use agents::instance::{self, InstanceManager};
use config::AideConfig;

/// Print a checklist step and bail on failure (fail-fast).
macro_rules! step {
    ($ok:expr, $label:expr) => {
        if $ok {
            println!("  ✓ {}", $label);
        } else {
            println!("  ✗ {}", $label);
            anyhow::bail!("{}", $label);
        }
    };
}

/// Print a checklist step — warn on failure but continue.
macro_rules! step_warn {
    ($ok:expr, $pass:expr, $fail:expr) => {
        if $ok {
            println!("  ✓ {}", $pass);
        } else {
            println!("  ⚠ {}", $fail);
        }
    };
}

#[derive(Parser)]
#[command(name = "aide", about = "aide — distributed autonomous agent runtime", version)]
struct Cli {
    /// Path to aide.toml config file
    #[arg(short, long, default_value = "aide.toml")]
    config: PathBuf,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    // ─── Container lifecycle ───

    /// Create and start an agent instance from an image
    Run {
        /// Agent image: <type> or <user>/<type>
        image: String,
        /// Instance name (default: <type>.<user>)
        #[arg(long)]
        name: Option<String>,
        /// Detach (run in background) — default for agents
        #[arg(short, long)]
        detach: bool,
    },
    /// Execute a command in a running agent instance
    Exec {
        /// Interactive mode (allocate pseudo-TTY)
        #[arg(short = 'i', long = "interactive")]
        interactive: bool,
        /// Allocate pseudo-TTY
        #[arg(short = 't', long = "tty")]
        tty: bool,
        /// Standalone mode: pipe query through LLM
        #[arg(short = 'p', long = "prompt")]
        prompt_mode: bool,
        /// Instance name
        instance: String,
        /// Command to execute (skill + args)
        command: Vec<String>,
    },
    /// List running agent instances
    Ps {
        /// Show all instances (including stopped)
        #[arg(short, long)]
        all: bool,
        /// Filter by organization
        #[arg(long)]
        org: Option<String>,
    },
    /// Stop a running agent instance
    Stop {
        /// Instance name(s)
        instance: Vec<String>,
    },
    /// Remove an agent instance
    Rm {
        /// Instance name(s)
        instance: Vec<String>,
        /// Force removal
        #[arg(short, long)]
        force: bool,
        /// Keep memory volumes
        #[arg(short = 'v', long = "keep-volumes")]
        keep_volumes: bool,
    },
    /// Fetch agent logs
    Logs {
        /// Instance name
        instance: String,
        /// Number of lines to show from end
        #[arg(long, default_value = "20")]
        tail: usize,
        /// Follow log output
        #[arg(short, long)]
        follow: bool,
    },
    /// Display detailed information on an agent instance
    Inspect {
        /// Instance name
        instance: String,
    },

    // ─── Image management ───

    /// Build an agent image from an Agentfile
    Build {
        /// Path to directory containing Agentfile.toml
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Tag the image (name:version)
        #[arg(short, long)]
        tag: Option<String>,
    },
    /// List locally available agent images
    Images,
    /// Push an agent image to the registry
    Push {
        /// Image to push (directory or name)
        #[arg(default_value = ".")]
        image: PathBuf,
        /// Push to private hub (requires login)
        #[arg(long)]
        private: bool,
    },
    /// Pull an agent image from the registry
    Pull {
        /// Image reference: <user>/<type>[:version]
        image: String,
    },
    /// Search the agent registry
    Search {
        /// Search query
        query: String,
    },
    /// Log in to the agent registry
    Login,
    /// Manage hub sources (git-native agent registry)
    Hub {
        #[command(subcommand)]
        action: HubAction,
    },

    // ─── System ───

    /// Display system-wide information
    Info,
    /// Start the aide daemon
    Up {
        /// Disable dashboard web UI
        #[arg(long)]
        no_dash: bool,
    },
    /// Stop the aide daemon
    Down,

    /// Run instance readiness checks (integration test for any instance)
    ///
    /// Validates: git repo, remote, vault keys, skills executable,
    /// cron registered, github polling, logs writable, Agentfile valid.
    Doctor {
        /// Instance name (checks all if omitted)
        instance: Option<String>,
        /// Filter by organization
        #[arg(long)]
        org: Option<String>,
    },

    /// Migrate pre-#72 instances to git-native format (idempotent)
    ///
    /// For each instance without .git:
    ///   1. git init
    ///   2. git remote add origin (from instance.toml github_repo, or creates gh repo)
    ///   3. git add -A && git commit
    ///   4. git push -u origin main
    ///
    /// Safe to run multiple times — skips already-migrated instances.
    Migrate {
        /// Instance name to migrate (migrates all if omitted)
        instance: Option<String>,
    },

    // ─── Agent-specific extensions ───

    /// Manage cron schedules for an agent
    Cron {
        #[command(subcommand)]
        action: CronAction,
    },
    /// Mount agent persona/memory into a CLI tool
    Mount {
        /// Instance name
        instance: String,
        /// Target: claude, codex, gemini, all
        target: String,
    },
    /// Unmount agent from a CLI tool
    Unmount {
        /// Instance name
        instance: String,
        /// Target: claude, codex, gemini, all
        target: String,
    },
    /// Manage encrypted credential vault
    Vault {
        #[command(subcommand)]
        action: VaultAction,
    },
    /// Sync vault/skills/memory across machines
    Sync {
        #[command(subcommand)]
        target: SyncTarget,
    },

    /// Initialize a new agent project scaffold
    Init {
        /// Agent name
        name: String,
    },
    /// Validate an Agentfile.toml (lint checks)
    Lint {
        /// Path to agent directory (default: current directory)
        #[arg(default_value = ".")]
        path: PathBuf,
    },
    /// Start MCP stdio server for LLM tool integration
    Mcp,
    /// Open the agent observability dashboard
    Dash {
        /// Port to serve on
        #[arg(short, long, default_value = "3939")]
        port: u16,
    },
    /// Live terminal dashboard (htop for agents)
    Top,
    /// Show current user info
    Whoami,
    /// Show usage cost summary
    Cost,
    /// Set up MCP integration for Claude Code / Codex
    SetupMcp {
        /// Target: claude (default)
        #[arg(default_value = "claude")]
        target: String,
    },
    /// Deploy an agent to GitHub with issue-driven workflow
    Deploy {
        /// Instance name
        instance: String,
        /// Create GitHub repo and push
        #[arg(long)]
        github: bool,
        /// Make the repo private
        #[arg(long)]
        private: bool,
    },
    /// Commit agent state (cognition/) to its GITAW repo
    Commit {
        /// Instance name
        instance: String,
        /// Commit message
        #[arg(short, long, default_value = "aide commit")]
        message: String,
    },
    /// Remove all aide data (~/.aide/) for clean reinstall
    Clean {
        /// Also remove vault keys (dangerous)
        #[arg(long)]
        include_vault: bool,
    },

    // ─── Hidden aliases for backward compat ───

    /// Alias for 'run'
    #[command(hide = true)]
    Spawn {
        agent_type: String,
        #[arg(long)]
        name: Option<String>,
    },
    /// Alias for 'exec'
    #[command(hide = true)]
    Call {
        instance: String,
        skill: Vec<String>,
    },
    /// Alias for 'info'
    #[command(hide = true)]
    Status,
    /// Alias for 'info'
    #[command(hide = true)]
    Check,
}

#[derive(Subcommand)]
enum CronAction {
    /// Add a cron job
    Add {
        /// Instance name
        instance: String,
        /// Cron schedule (e.g. "*/5 * * * *")
        schedule: String,
        /// Skill to run
        skill: String,
    },
    /// Remove a cron job
    Rm {
        /// Instance name
        instance: String,
        /// Skill to remove
        skill: String,
    },
    /// List cron jobs
    Ls {
        /// Instance name
        instance: String,
    },
}

#[derive(Subcommand)]
enum SyncTarget {
    Vault,
    Skills,
    Status,
}

#[derive(Subcommand)]
enum HubAction {
    /// Initialize a new hub repo
    Init {
        /// Hub name (creates {name} repo)
        name: String,
        /// Create as private repo
        #[arg(long)]
        private: bool,
    },
    /// Add a hub source
    Add {
        /// Repository (e.g. acme-corp/aide-hub)
        repo: String,
    },
    /// List configured hubs
    Ls,
    /// Remove a hub source
    Rm {
        /// Hub name to remove
        name: String,
    },
}

#[derive(Subcommand)]
enum VaultAction {
    /// Import env file into encrypted vault
    Import {
        path: PathBuf,
    },
    /// Set one or more secrets: KEY=VALUE [KEY2=VALUE2 ...]
    Set {
        /// Key=value pairs
        pairs: Vec<String>,
    },
    /// Rotate vault key (re-encrypt with new keypair)
    Rotate,
    /// Show vault status
    Status,
    /// Store a registry token in the vault
    SetToken {
        /// Username
        username: String,
        /// Token value
        token: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();

    // Commands that don't require aide.toml
    match &cli.command {
        Command::Build { path, tag: _ } => return cmd_build(path),
        Command::Push { image, private: _ } => {
            return cmd_push(image);
        }
        Command::Pull { image } => {
            // If argument matches an existing instance with .git/, do git pull
            let config = AideConfig::load(&cli.config).unwrap_or_else(|_| AideConfig::default());
            let mgr = InstanceManager::new(&config.aide.data_dir);
            if let Ok(Some(_)) = mgr.get(image) {
                let inst_dir = mgr.base_dir().join(image);
                if inst_dir.join(".git").exists() {
                    return cmd_pull_instance(&inst_dir, image);
                }
            }
            let (agent_ref, _version) = parse_image_ref(image);
            return cmd_pull(&agent_ref);
        }
        Command::Login => return cmd_login().await,
        Command::Hub { action } => return cmd_hub(action),
        Command::Search { query } => return cmd_search(query),
        Command::Images => return cmd_images(),
        Command::Init { name } => return cmd_init(name),
        Command::Lint { path } => return cmd_lint(path),
        Command::Mcp => {
            let config = AideConfig::load(&cli.config).unwrap_or_else(|_| AideConfig::default());
            return mcp::run_mcp_server(&config.aide.data_dir);
        }
        Command::Dash { port } => {
            let config = AideConfig::load(&cli.config).unwrap_or_else(|_| AideConfig::default());
            return cmd_dash(&config.aide.data_dir, *port).await;
        }
        Command::Top => {
            let config = AideConfig::load(&cli.config).unwrap_or_else(|_| AideConfig::default());
            return top::run_top(&config.aide.data_dir);
        }
        Command::SetupMcp { target } => return cmd_setup_mcp(target),
        Command::Deploy { instance, github, private } => {
            if *github {
                let config = AideConfig::load(&cli.config).unwrap_or_else(|_| AideConfig::default());
                return cmd_deploy_github(&config.aide.data_dir, instance, *private);
            } else {
                bail!("specify --github to deploy to GitHub");
            }
        }
        Command::Whoami => return cmd_whoami(),
        Command::Commit { instance, message } => {
            let config = AideConfig::load(&cli.config).unwrap_or_else(|_| AideConfig::default());
            return cmd_commit(&config.aide.data_dir, instance, message);
        }
        Command::Clean { include_vault } => return cmd_clean(*include_vault),
        Command::Cost => {
            let config = AideConfig::load(&cli.config).unwrap_or_else(|_| AideConfig::default());
            return cmd_cost(&config.aide.data_dir);
        }
        _ => {}
    }

    // Load config with fallback to defaults (no aide.toml needed for basic usage)
    let config = AideConfig::load(&cli.config).unwrap_or_else(|_| AideConfig::default());
    let mgr = InstanceManager::new(&config.aide.data_dir);

    match cli.command {
        // ─── Lifecycle ───
        Command::Run { image, name, .. } | Command::Spawn { agent_type: image, name } => {
            cmd_run(&config, &mgr, &image, name.as_deref())?;
        }
        Command::Exec { instance, command, interactive, tty, prompt_mode } => {
            let skill = command.join(" ");
            if prompt_mode {
                cmd_exec_prompt(&mgr, &instance, &skill)?;
            } else {
                cmd_exec(&mgr, &instance, &skill, interactive || tty)?;
            }
        }
        Command::Call { instance, skill } => {
            cmd_exec(&mgr, &instance, &skill.join(" "), false)?;
        }
        Command::Ps { all: _, org } => {
            cmd_ps(&mgr, org.as_deref())?;
        }
        Command::Stop { instance } => {
            for inst in &instance {
                cmd_stop(&mgr, inst)?;
            }
        }
        Command::Rm { instance, keep_volumes, .. } => {
            for inst in &instance {
                cmd_rm(&mgr, inst, keep_volumes)?;
            }
        }
        Command::Logs { instance, tail, follow } => {
            cmd_logs(&mgr, &instance, tail, follow)?;
        }
        Command::Inspect { instance } => {
            cmd_inspect(&mgr, &instance)?;
        }

        // ─── Images ───
        Command::Images => {
            cmd_images()?;
        }

        // ─── System ───
        Command::Info | Command::Status | Command::Check => {
            cmd_info(&config, &mgr)?;
        }
        Command::Up { no_dash } => {
            let d = daemon::Daemon::new(config).with_dash(!no_dash);
            d.run().await?;
        }
        Command::Down => {
            println!("aide daemon stopping...");
            match daemon::stop_daemon()? {
                true => println!("aide daemon stopped."),
                false => println!("no running daemon found."),
            }
        }

        Command::Doctor { instance, org } => {
            cmd_doctor(&mgr, instance.as_deref(), org.as_deref())?;
        }

        Command::Migrate { instance } => {
            cmd_migrate(&mgr, instance.as_deref())?;
        }

        // ─── Agent extensions ───
        Command::Cron { action } => match action {
            CronAction::Add { instance, schedule, skill } => {
                mgr.cron_add(&instance, &schedule, &skill)?;
                println!("cron added: {} → {} ({})", instance, skill, schedule);
            }
            CronAction::Rm { instance, skill } => {
                if mgr.cron_rm(&instance, &skill)? {
                    println!("cron removed: {} → {}", instance, skill);
                } else {
                    println!("cron entry '{}' not found on {}", skill, instance);
                }
            }
            CronAction::Ls { instance } => {
                cmd_cron_ls(&mgr, &instance)?;
            }
        },
        Command::Mount { instance, target } => {
            cmd_mount(&mgr, &instance, &target)?;
        }
        Command::Unmount { instance, target } => {
            cmd_unmount(&mgr, &instance, &target)?;
        }
        Command::Vault { action } => {
            let vault_repo = config
                .aide
                .vault_repo
                .as_ref()
                .map(|p| shellexpand::tilde(p).to_string())
                .unwrap_or_else(|| {
                    // Fallback: legacy vault_path location or default
                    config
                        .aide
                        .vault_path
                        .as_ref()
                        .map(|p| shellexpand::tilde(p).to_string())
                        .map(|p| {
                            // If vault_path points to a file, use its parent dir
                            let path = PathBuf::from(&p);
                            path.parent()
                                .unwrap_or(&path)
                                .to_string_lossy()
                                .to_string()
                        })
                        .unwrap_or_else(|| {
                            // Default to ~/.aide so vault.age lives at ~/.aide/vault.age
                            let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
                            format!("{}/.aide", home)
                        })
                });
            let v = vault::Vault::from_config(&vault_repo, None);

            match action {
                VaultAction::Import { path } => {
                    v.import_env(&path).await?;
                    fix_vault_key_permissions(&v.identity_path());
                    println!("imported {} → vault", path.display());
                }
                VaultAction::Set { pairs } => {
                    cmd_vault_set(&v, &pairs).await?;
                }
                VaultAction::Rotate => {
                    cmd_vault_rotate(&v).await?;
                }
                VaultAction::Status => {
                    cmd_vault_status(&v).await;
                }
                VaultAction::SetToken { username, token } => {
                    cmd_vault_set_token(&v, &username, &token).await?;
                }
            }
        }
        Command::Sync { target } => match target {
            SyncTarget::Vault => { sync::sync_vault(&config).await?; }
            SyncTarget::Skills => { sync::sync_skills(&config).await?; }
            SyncTarget::Status => { sync::sync_status(&config).await?; }
        },

        // These are handled above before config load
        Command::Build { .. }
        | Command::Push { .. }
        | Command::Pull { .. }
        | Command::Login
        | Command::Hub { .. }
        | Command::Search { .. }
        | Command::Init { .. }
        | Command::Lint { .. }
        | Command::Mcp
        | Command::Dash { .. }
        | Command::Top
        | Command::SetupMcp { .. }
        | Command::Deploy { .. }
        | Command::Whoami
        | Command::Commit { .. }
        | Command::Clean { .. }
        | Command::Cost => unreachable!(),
    }

    Ok(())
}

// ─── aide migrate ────────────────────────────────────────────────────────────

/// Migrate pre-#72 instances to git-native format. Idempotent.
///
/// For each instance:
///   1. Skip if .git already exists
///   2. git init
///   3. Set remote: use github_repo from instance.toml if present,
///      otherwise create yiidtw/aide-<name> via `gh repo create`
///   4. git add -A && git commit -m "chore: migrate to git-native instance"
///   5. git push -u origin main
fn cmd_migrate(mgr: &InstanceManager, instance: Option<&str>) -> Result<()> {
    let names: Vec<String> = match instance {
        Some(n) => vec![n.to_string()],
        None => mgr.list()?.into_iter().map(|i| i.name).collect(),
    };

    if names.is_empty() {
        println!("no instances found");
        return Ok(());
    }

    for name in &names {
        println!("── migrate: {} ──", name);
        let inst_dir = mgr.base_dir().join(name);
        if !inst_dir.exists() {
            println!("  ✗ instance directory not found");
            continue;
        }

        // Step 1: idempotent check
        let has_git = inst_dir.join(".git").exists();
        if has_git {
            println!("  ✓ git repo exists");
            let has_remote = std::process::Command::new("git")
                .args(["remote", "get-url", "origin"])
                .current_dir(&inst_dir)
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false);
            if has_remote {
                println!("  ✓ remote origin configured");
                println!("  ── already migrated");
            } else {
                println!("  ✗ remote origin missing — setting up");
                match migrate_set_remote(mgr, name, &inst_dir) {
                    Ok(()) => println!("  ✓ remote origin configured"),
                    Err(e) => println!("  ✗ remote setup failed: {}", e),
                }
            }
            continue;
        }

        // Step 2: git init (critical — bail on fail)
        let git_init_ok = std::process::Command::new("git")
            .args(["init", "-b", "main"])
            .current_dir(&inst_dir)
            .status()
            .context("git init failed")?
            .success();
        if !git_init_ok {
            println!("  ✗ git init");
            continue;
        }
        println!("  ✓ git init");

        // Step 3: set remote (non-critical — warn and continue)
        match migrate_set_remote(mgr, name, &inst_dir) {
            Ok(()) => println!("  ✓ remote origin configured"),
            Err(e) => println!("  ⚠ remote setup: {} (continuing without push)", e),
        }

        // Step 4: initial commit (critical — bail on fail)
        std::process::Command::new("git")
            .args(["add", "-A"])
            .current_dir(&inst_dir)
            .status()
            .ok();

        let has_staged = std::process::Command::new("git")
            .args(["diff", "--cached", "--quiet"])
            .current_dir(&inst_dir)
            .status()
            .map(|s| !s.success())
            .unwrap_or(false);

        if has_staged {
            let committed = std::process::Command::new("git")
                .args(["commit", "-m", "chore: migrate to git-native instance"])
                .current_dir(&inst_dir)
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
            if !committed {
                println!("  ✗ commit failed");
                continue;
            }
            println!("  ✓ initial commit");
        } else {
            println!("  ✓ nothing to commit (clean)");
        }

        // Step 5: push (non-critical — warn on fail)
        let has_remote = std::process::Command::new("git")
            .args(["remote", "get-url", "origin"])
            .current_dir(&inst_dir)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

        if has_remote {
            let pushed = std::process::Command::new("git")
                .args(["push", "-u", "origin", "main"])
                .current_dir(&inst_dir)
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
            step_warn!(pushed, "pushed to origin/main", "push failed — check remote auth");
        } else {
            println!("  ⚠ no remote — skipping push");
        }
    }

    Ok(())
}

/// Resolve or create the GitHub remote for an instance.
/// Uses github_repo from instance.toml if present; otherwise creates
/// yiidtw/aide-<name> via `gh repo create`.
fn migrate_set_remote(mgr: &InstanceManager, name: &str, inst_dir: &Path) -> Result<()> {
    // Try to get repo from manifest
    let repo = mgr.get(name)
        .ok()
        .flatten()
        .and_then(|m| m.github_repo);

    let repo = match repo {
        Some(r) => r,
        None => {
            // Create a new GitHub repo
            let gh_name = format!("aide-{}", name);
            let output = std::process::Command::new("gh")
                .args(["repo", "create", &gh_name, "--private", "--confirm"])
                .output()
                .context("gh not found — install GitHub CLI")?;
            if !output.status.success() {
                let err = String::from_utf8_lossy(&output.stderr);
                bail!("gh repo create failed: {}", err.trim());
            }
            // gh outputs the repo URL; extract owner/repo
            let stdout = String::from_utf8_lossy(&output.stdout);
            // Store in manifest
            let full = stdout.trim()
                .trim_start_matches("https://github.com/")
                .to_string();
            // Update instance.toml with new github_repo
            if let Ok(Some(mut manifest)) = mgr.get(name) {
                manifest.github_repo = Some(full.clone());
                let manifest_path = mgr.base_dir().join(name).join("cognition").join("instance.toml");
                if let Ok(content) = toml::to_string_pretty(&manifest) {
                    let _ = std::fs::write(&manifest_path, content);
                }
            }
            full
        }
    };

    let remote_url = if repo.starts_with("https://") || repo.starts_with("git@") {
        repo
    } else {
        format!("git@github.com:{}.git", repo)
    };

    let ok = std::process::Command::new("git")
        .args(["remote", "add", "origin", &remote_url])
        .current_dir(inst_dir)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if !ok {
        bail!("git remote add origin {} failed", remote_url);
    }

    Ok(())
}

// ─── aide doctor ─────────────────────────────────────────────────────────────

/// Run instance readiness checks. This is the generalized integration test
/// for any aide instance — validates that all lifecycle features are functional.
fn cmd_doctor(mgr: &InstanceManager, instance: Option<&str>, org_filter: Option<&str>) -> Result<()> {
    let names: Vec<String> = match instance {
        Some(n) => vec![n.to_string()],
        None => {
            let mut list: Vec<String> = mgr.list()?.into_iter().map(|i| i.name).collect();
            if let Some(org) = org_filter {
                // Filter by org: need to load each manifest to check org field
                list.retain(|name| {
                    mgr.get(name).ok().flatten()
                        .and_then(|m| m.org)
                        .as_deref() == Some(org)
                });
            }
            list
        }
    };

    if names.is_empty() {
        println!("no instances found");
        return Ok(());
    }

    let vault_env = daemon_load_vault_env().unwrap_or_default();
    let mut total_pass = 0usize;
    let mut total_fail = 0usize;

    for name in &names {
        println!("── {} ──", name);
        let inst_dir = mgr.base_dir().join(name);
        if !inst_dir.exists() {
            println!("  ✗ instance directory not found");
            total_fail += 1;
            continue;
        }

        let mut pass = 0usize;
        let mut fail = 0usize;

        // 1. Git repo initialized
        let has_git = inst_dir.join(".git").exists();
        check(&mut pass, &mut fail, has_git, "git repo initialized");

        // 2. Git remote set
        let has_remote = std::process::Command::new("git")
            .args(["remote", "get-url", "origin"])
            .current_dir(&inst_dir)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        check(&mut pass, &mut fail, has_remote, "git remote origin configured");

        // 3. Agentfile.toml exists and parses
        let spec = AgentfileSpec::load(&inst_dir);
        let spec_ok = spec.is_ok();
        check(&mut pass, &mut fail, spec_ok, "Agentfile.toml valid");

        if let Ok(ref spec) = spec {
            // 4. Persona file exists
            let has_persona = spec.persona.as_ref().map(|p| {
                let base = AgentfileSpec::base_dir(&inst_dir);
                base.join(&p.file).exists()
            }).unwrap_or(false);
            check(&mut pass, &mut fail, has_persona, "persona.md exists");

            // 5. All skill scripts exist
            let mut all_skills_ok = true;
            for (skill_name, skill_def) in &spec.skills {
                let base = AgentfileSpec::base_dir(&inst_dir);
                let script_ok = skill_def.script.as_ref().map(|s| base.join(s).exists()).unwrap_or(false)
                    || skill_def.prompt.as_ref().map(|p| base.join(p).exists()).unwrap_or(false);
                if !script_ok {
                    println!("  ✗ skill '{}' script missing", skill_name);
                    all_skills_ok = false;
                    fail += 1;
                }
            }
            if all_skills_ok && !spec.skills.is_empty() {
                check(&mut pass, &mut fail, true, &format!("all {} skill scripts exist", spec.skills.len()));
            }

            // 6. Vault keys available for required env
            if let Some(ref env_section) = spec.env {
                let mut missing_keys = Vec::new();
                for required_key in &env_section.required {
                    let found = vault_env.iter().any(|(k, _)| k == required_key)
                        || std::env::var(required_key).is_ok();
                    if !found {
                        missing_keys.push(required_key.as_str());
                    }
                }
                let vault_ok = missing_keys.is_empty();
                if vault_ok {
                    check(&mut pass, &mut fail, true, &format!("vault: {} required keys present", env_section.required.len()));
                } else {
                    println!("  ✗ vault: missing keys: {}", missing_keys.join(", "));
                    fail += 1;
                }
            }

            // 7. Limits configured
            if let Some(ref limits) = spec.limits {
                check(&mut pass, &mut fail, true, &format!("limits: timeout={}s retry={}", limits.max_timeout, limits.max_retry));
            } else {
                println!("  ⚠ limits: not configured (defaults: timeout=300s retry=0)");
            }

            // 8. GitHub expose configured
            let has_github_expose = spec.expose.as_ref()
                .and_then(|e| e.github.as_ref())
                .is_some();
            let has_github_repo = mgr.get(name).ok().flatten()
                .and_then(|m| m.github_repo).is_some();
            let github_ok = has_github_expose || has_github_repo;
            check(&mut pass, &mut fail, github_ok, "github issue polling configured");
        }

        // 9. Cron entries registered
        let cron_entries = mgr.cron_list(name).unwrap_or_default();
        if !cron_entries.is_empty() {
            check(&mut pass, &mut fail, true, &format!("{} cron entries registered", cron_entries.len()));
        } else {
            println!("  ⚠ no cron entries (manual-only instance)");
        }

        // 10. Cognition dirs exist
        let has_cognition = inst_dir.join("cognition/memory").exists()
            && inst_dir.join("cognition/logs").exists();
        check(&mut pass, &mut fail, has_cognition, "cognition/ directory structure");

        // 11. Logs writable
        let log_test = mgr.append_log(name, "doctor: readiness check");
        check(&mut pass, &mut fail, log_test.is_ok(), "logs writable");

        // 12. Daemon running
        let daemon_up = {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
            let pid_path = PathBuf::from(&home).join(".aide").join("daemon.pid");
            pid_path.exists()
        };
        check(&mut pass, &mut fail, daemon_up, "daemon running");

        println!("  ── {}/{} checks passed", pass, pass + fail);
        total_pass += pass;
        total_fail += fail;
    }

    if names.len() > 1 {
        println!("\n═══ total: {}/{} checks passed across {} instances",
            total_pass, total_pass + total_fail, names.len());
    }

    if total_fail > 0 {
        std::process::exit(1);
    }
    Ok(())
}

fn check(pass: &mut usize, fail: &mut usize, ok: bool, label: &str) {
    if ok {
        println!("  ✓ {}", label);
        *pass += 1;
    } else {
        println!("  ✗ {}", label);
        *fail += 1;
    }
}

/// Print config location hints after instance creation.
fn print_config_hints(inst_dir: &Path, instance_name: &str) {
    let agentfile = if inst_dir.join("occupation/Agentfile.toml").exists() {
        inst_dir.join("occupation/Agentfile.toml")
    } else {
        inst_dir.join("Agentfile.toml")
    };

    // Read current limits to show what's configured
    let limits_info = AgentfileSpec::load(inst_dir)
        .ok()
        .and_then(|spec| spec.limits)
        .map(|l| format!("timeout={}s, retry={}, tokens={}", l.max_timeout, l.max_retry, l.max_tokens))
        .unwrap_or_else(|| "not set (defaults: timeout=300s, retry=0, tokens=4096)".to_string());

    println!();
    println!("  configure:");
    println!("    {}  — skills, env, limits, expose", agentfile.display());
    println!("    limits: {}", limits_info);
    println!("    aide.toml [aide] daily_commit_hour = 3  — daily cognition commit (local time)");
    println!();
    println!("  next:");
    println!("    aide deploy --github {}  — create GitHub repo + enable issue polling", instance_name);
    println!("    aide cron add {} \"0 8 * * *\" <skill>  — schedule a skill", instance_name);
    println!("    aide doctor {}  — validate readiness", instance_name);
    println!("    aide up  — start daemon (restart after config changes)");
}

/// Re-export vault loading for doctor command
fn daemon_load_vault_env() -> Result<Vec<(String, String)>> {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let aide_home = PathBuf::from(home).join(".aide");
    let vault_path = aide_home.join("vault.age");
    if !vault_path.exists() { return Ok(Vec::new()); }
    let identity_path = aide_home.join("vault.key");
    if !identity_path.exists() { return Ok(Vec::new()); }
    let output = std::process::Command::new("age")
        .args(["-d", "-i"])
        .arg(&identity_path)
        .arg(&vault_path)
        .output()?;
    if !output.status.success() { return Ok(Vec::new()); }
    let content = String::from_utf8_lossy(&output.stdout);
    let mut vars = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') { continue; }
        let line = line.strip_prefix("export ").unwrap_or(line);
        if let Some((key, val)) = line.split_once('=') {
            let val = val.trim_matches('"').trim_matches('\'');
            vars.push((key.to_string(), val.to_string()));
        }
    }
    Ok(vars)
}

// ─── Parse image ref: "user/type:version" → ("user/type", "version") ───

fn parse_image_ref(image: &str) -> (String, String) {
    if let Some((ref_part, version)) = image.rsplit_once(':') {
        (ref_part.to_string(), version.to_string())
    } else {
        (image.to_string(), "latest".to_string())
    }
}

// ─── MCP setup ───

fn cmd_setup_mcp(target: &str) -> Result<()> {
    match target {
        "claude" => setup_mcp_claude(),
        _ => bail!("unknown target '{}'. Supported: claude", target),
    }
}

fn setup_mcp_claude() -> Result<()> {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let settings_path = PathBuf::from(&home).join(".claude").join("settings.json");

    // Find aide binary path
    let exe_path = find_aide_binary();

    // Read or create settings
    let mut settings: serde_json::Value = if settings_path.exists() {
        let content = std::fs::read_to_string(&settings_path)?;
        serde_json::from_str(&content).unwrap_or(serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    // Add MCP server config
    let mcp_servers = settings
        .as_object_mut()
        .context("settings is not a JSON object")?
        .entry("mcpServers")
        .or_insert(serde_json::json!({}));

    mcp_servers
        .as_object_mut()
        .context("mcpServers is not a JSON object")?
        .insert(
            "aide".to_string(),
            serde_json::json!({
                "command": exe_path,
                "args": ["mcp"]
            }),
        );

    // Write back
    std::fs::create_dir_all(settings_path.parent().unwrap())?;
    std::fs::write(&settings_path, serde_json::to_string_pretty(&settings)?)?;

    println!("MCP server configured for Claude Code");
    println!("  settings: {}", settings_path.display());
    println!("  command: {} mcp", exe_path);
    println!();
    println!("Restart Claude Code to activate.");
    Ok(())
}

fn find_aide_binary() -> String {
    // Try current exe first
    if let Ok(exe) = std::env::current_exe() {
        return exe.to_string_lossy().to_string();
    }
    // Fallback
    "aide".to_string()
}

// ─── Command implementations ───

fn cmd_ps(mgr: &InstanceManager, org_filter: Option<&str>) -> Result<()> {
    let mut instances = mgr.list()?;
    if let Some(org) = org_filter {
        instances.retain(|i| i.org.as_deref() == Some(org));
    }
    if instances.is_empty() {
        if let Some(org) = org_filter {
            println!("No instances in org '{}'. Use `aide ps` to see all.", org);
        } else {
            println!("No agent instances. Use `aide run <image>` to create one.");
        }
        return Ok(());
    }

    // Group by org for display
    let mut by_org: std::collections::BTreeMap<String, Vec<&instance::InstanceInfo>> = std::collections::BTreeMap::new();
    for inst in &instances {
        let org = inst.org.as_deref().unwrap_or("—").to_string();
        by_org.entry(org).or_default().push(inst);
    }

    for (org, members) in &by_org {
        println!("── {} ──", org);
        for inst in members {
            let last = inst.last_activity.as_deref().unwrap_or("—");
            // Truncate last activity
            let last: String = last.chars().take(50).collect();
            let is_router = inst.org_router.as_deref() == Some(&inst.name);
            let role = if is_router { "★" } else { " " };
            println!(
                "  {} {:<24} {:<8} {:<6} {}",
                role,
                inst.name,
                inst.status,
                inst.cron_count,
                last
            );
        }
        println!();
    }

    Ok(())
}

fn cmd_run(
    config: &AideConfig,
    mgr: &InstanceManager,
    image: &str,
    name: Option<&str>,
) -> Result<()> {
    // Check if this is a pulled type reference (user/type)
    if image.contains('/') {
        return cmd_run_from_pulled(mgr, image, name);
    }

    let def = config.agents.get(image).ok_or_else(|| {
        let available: Vec<_> = config.agents.keys().cloned().collect();
        anyhow::anyhow!(
            "image '{}' not found. Available: {}\nUse `aide pull <user>/{}` to fetch from registry.",
            image,
            available.join(", "),
            image
        )
    })?;

    let instance_name = match name {
        Some(n) => n.to_string(),
        None => instance::default_instance_name(image),
    };

    println!("── run: {} ──", instance_name);
    let manifest = mgr.spawn(image, &instance_name, def)?;
    step!(true, format!("instance created from image '{}'", image));
    step!(mgr.base_dir().join(&instance_name).join(".git").exists(), "git repo initialized");
    step!(mgr.append_log(&instance_name, &format!("created from image '{}'", image)).is_ok(), "log initialized");

    let inst_dir = mgr.base_dir().join(&instance_name);
    print_config_hints(&inst_dir, &manifest.name);

    Ok(())
}

fn cmd_run_from_pulled(
    mgr: &InstanceManager,
    image: &str,
    name: Option<&str>,
) -> Result<()> {
    let (agent_ref, _version) = parse_image_ref(image);
    let parts: Vec<&str> = agent_ref.splitn(2, '/').collect();
    if parts.len() != 2 {
        bail!("invalid image '{}' — expected <user>/<type>", image);
    }
    let (user, agent_type) = (parts[0], parts[1]);

    let types_dir = aide_home().join("types").join(user).join(agent_type);
    if !types_dir.exists() {
        bail!(
            "image '{}' not found locally. Run `aide pull {}` first.",
            image, image
        );
    }

    let spec = AgentfileSpec::load(&types_dir)?;

    let instance_name = match name {
        Some(n) => n.to_string(),
        None => instance::default_instance_name(agent_type),
    };

    println!("── run: {} ──", instance_name);

    // Git clone path: if Agentfile has [expose.github] repo, clone it directly
    if let Some(expose) = &spec.expose {
        if let Some(gh) = &expose.github {
            return cmd_run_from_github_clone(mgr, &gh.repo, &instance_name, agent_type, image);
        }
    }

    let def = config::AgentDef {
        email: format!("{}@aide.sh", spec.agent.name),
        role: spec.agent.description.clone().unwrap_or_default(),
        domains: Vec::new(),
        persona_path: spec.persona.as_ref().map(|p| {
            types_dir.join(&p.file).to_string_lossy().to_string()
        }),
    };

    let manifest = mgr.spawn(agent_type, &instance_name, &def)?;
    println!("  ✓ instance spawned");
    mgr.append_log(&instance_name, &format!("created from image '{}'", image))?;

    // Copy type files into occupation/ (image manifest travels with container)
    let inst_dir = mgr.base_dir().join(&instance_name);
    let occ_dir = inst_dir.join("occupation");
    std::fs::create_dir_all(&occ_dir)?;

    // Resolve types_dir base (may have occupation/ subdir from new-format types)
    let types_base = AgentfileSpec::base_dir(&types_dir);

    let agentfile_src = types_base.join("Agentfile.toml");
    if agentfile_src.exists() {
        std::fs::copy(&agentfile_src, occ_dir.join("Agentfile.toml"))?;
    }
    println!("  ✓ Agentfile.toml copied");

    // Copy skill files into occupation/skills/
    let skills_dir = occ_dir.join("skills");
    std::fs::create_dir_all(&skills_dir)?;

    let mut skill_count = 0usize;
    let mut cron_count = 0usize;
    for (skill_name, skill_def) in &spec.skills {
        if let Some(script) = &skill_def.script {
            let src = types_base.join(script);
            if src.exists() {
                let dst = skills_dir.join(
                    Path::new(script)
                        .file_name()
                        .unwrap_or(std::ffi::OsStr::new(skill_name)),
                );
                std::fs::copy(&src, &dst)?;
                skill_count += 1;
            }
        }
        if let Some(prompt) = &skill_def.prompt {
            let src = types_base.join(prompt);
            if src.exists() {
                let dst = skills_dir.join(
                    Path::new(prompt)
                        .file_name()
                        .unwrap_or(std::ffi::OsStr::new(skill_name)),
                );
                std::fs::copy(&src, &dst)?;
                skill_count += 1;
            }
        }
        if let Some(schedule) = &skill_def.schedule {
            mgr.cron_add(&instance_name, schedule, skill_name)?;
            cron_count += 1;
        }
    }
    println!("  ✓ {} skills copied", skill_count);
    if cron_count > 0 {
        println!("  ✓ {} cron entries registered", cron_count);
    }

    // Copy knowledge data into occupation/knowledge/
    if let Some(knowledge) = &spec.knowledge {
        let knowledge_src = types_base.join(&knowledge.dir);
        if knowledge_src.exists() {
            let knowledge_dst = occ_dir.join("knowledge");
            copy_dir_recursive(&knowledge_src, &knowledge_dst)?;
            println!("  ✓ knowledge copied");
        }
    }

    // Ensure cognition/ dirs exist
    std::fs::create_dir_all(inst_dir.join("cognition/memory"))?;
    std::fs::create_dir_all(inst_dir.join("cognition/logs"))?;
    println!("  ✓ cognition/ initialized");
    println!("  ✓ git repo initialized");

    print_config_hints(&inst_dir, &instance_name);

    Ok(())
}

/// Create an instance by cloning a GitHub repo directly.
/// The instance directory IS the git working tree.
fn cmd_run_from_github_clone(
    mgr: &InstanceManager,
    github_repo: &str,
    instance_name: &str,
    agent_type: &str,
    image: &str,
) -> Result<()> {
    let inst_dir = mgr.base_dir().join(instance_name);
    if inst_dir.exists() {
        bail!(
            "instance '{}' already exists. Use `aide rm {}` first.",
            instance_name, instance_name
        );
    }

    // Note: "── run:" header already printed by caller

    let clone_output = std::process::Command::new("git")
        .args([
            "clone",
            &format!("git@github.com:{}.git", github_repo),
            inst_dir.to_str().unwrap(),
        ])
        .output()?;

    step!(clone_output.status.success(), format!("cloned git@github.com:{}.git", github_repo));

    // Ensure cognition/logs/ exists (it's gitignored so won't be in the clone)
    std::fs::create_dir_all(inst_dir.join("cognition/logs"))?;
    std::fs::create_dir_all(inst_dir.join("cognition/memory"))?;
    println!("  ✓ cognition/ initialized");

    // Write/update instance.toml with local machine identity
    let manifest_path = inst_dir.join("cognition/instance.toml");
    let mut manifest = if manifest_path.exists() {
        let content = std::fs::read_to_string(&manifest_path)?;
        toml::from_str::<instance::InstanceManifest>(&content)
            .unwrap_or_else(|_| instance::InstanceManifest {
                name: instance_name.to_string(),
                agent_type: agent_type.to_string(),
                created_at: chrono::Utc::now(),
                email: format!("{}@aide.sh", agent_type),
                role: String::new(),
                domains: Vec::new(),
                cron: Vec::new(),
                github_repo: Some(github_repo.to_string()),
                uuid: Some(uuid::Uuid::new_v4().to_string()),
                machine_id: Some(instance::gethostname()),
                org: None,
                org_router: None,
            })
    } else {
        instance::InstanceManifest {
            name: instance_name.to_string(),
            agent_type: agent_type.to_string(),
            created_at: chrono::Utc::now(),
            email: format!("{}@aide.sh", agent_type),
            role: String::new(),
            domains: Vec::new(),
            cron: Vec::new(),
            github_repo: Some(github_repo.to_string()),
            uuid: Some(uuid::Uuid::new_v4().to_string()),
            machine_id: Some(instance::gethostname()),
            org: None,
            org_router: None,
        }
    };

    // Always update identity for this machine
    manifest.name = instance_name.to_string();
    manifest.machine_id = Some(instance::gethostname());
    if manifest.uuid.is_none() {
        manifest.uuid = Some(uuid::Uuid::new_v4().to_string());
    }
    manifest.github_repo = Some(github_repo.to_string());

    let content = toml::to_string_pretty(&manifest)?;
    std::fs::write(&manifest_path, content)?;
    println!("  ✓ instance manifest written (machine: {})", instance::gethostname());

    mgr.append_log(instance_name, &format!("created from git clone '{}' (image: {})", github_repo, image))?;

    // Auto-commit the machine_id/uuid update
    if agents::commit::auto_commit_instance(&inst_dir, &format!("run: {} on {}", instance_name, instance::gethostname())).is_some() {
        println!("  ✓ identity committed & pushed");
    }

    print_config_hints(&inst_dir, instance_name);

    Ok(())
}

fn cmd_exec(mgr: &InstanceManager, instance: &str, skill: &str, _interactive: bool) -> Result<()> {
    let manifest = match mgr.get(instance)? {
        Some(m) => m,
        None => {
            if instance.contains('/') {
                let suggested_name = instance.rsplit('/').next().unwrap_or(instance);
                bail!(
                    "Instance '{}' not found.\n\nTo create it:\n  aide pull {}\n  aide run {} --name {}\n  aide exec {} {}",
                    instance, instance, instance, suggested_name, suggested_name, skill
                );
            } else {
                bail!(
                    "No such instance: {}\n\nAvailable instances: aide ps\nTo pull from hub: aide pull <user>/{}",
                    instance, instance
                );
            }
        }
    };

    // Handle --help: show available skills from Agentfile
    if skill == "--help" || skill == "-h" || skill.is_empty() {
        let inst_dir = mgr.base_dir().join(instance);
        if let Ok(spec) = AgentfileSpec::load(&inst_dir) {
            print!("{}", spec.format_help(instance));
        } else {
            println!("{} (no Agentfile.toml — skill discovery unavailable)", instance);
            println!("\nUsage: aide exec {} <skill> [args...]", instance);
        }
        return Ok(());
    }

    // Parse skill input
    let parts: Vec<&str> = skill.splitn(2, ' ').collect();
    let skill_name = parts[0];
    let skill_args = if parts.len() > 1 { parts[1] } else { "" };

    eprintln!("── exec: {} {} ──", instance, skill_name);
    mgr.append_log(instance, &format!("exec: {}", skill))?;

    // Load scoped env (Docker secrets model — per-skill > per-agent > vault)
    let inst_dir = mgr.base_dir().join(instance);
    let scoped_env = load_scoped_env(&inst_dir, Some(skill_name))?;
    eprintln!("  ✓ vault env loaded");

    // Resolve and dispatch
    let script_found = resolve_skill_script(&inst_dir, skill_name);
    if script_found.is_some() {
        eprintln!("  ✓ skill script resolved");
    } else {
        eprintln!("  ⚠ no local script — trying wonskill");
    }

    let (exit_code, stdout, stderr) = if let Some(script) = script_found {
        exec_skill_script(&script, skill_args, &inst_dir, &scoped_env)?
    } else {
        exec_wonskill(skill_name, skill_args, &inst_dir, &scoped_env)?
    };

    // Output
    if !stdout.is_empty() {
        print!("{}", stdout);
        if !stdout.ends_with('\n') {
            println!();
        }
    }
    if !stderr.is_empty() {
        eprint!("{}", stderr);
    }

    // Log
    let status_msg = if exit_code == 0 { "ok" } else { "FAILED" };
    if exit_code == 0 {
        eprintln!("  ✓ exit 0");
    } else {
        eprintln!("  ✗ exit {}", exit_code);
    }
    mgr.append_log(
        instance,
        &format!("exec-result: {} → {} (exit {})", skill, status_msg, exit_code),
    )?;

    // Auto-commit if instance is a git repo (fire-and-forget)
    let inst_dir = mgr.base_dir().join(instance);
    if let Some(summary) = agents::commit::auto_commit_instance(&inst_dir, &format!("exec: {}", skill)) {
        eprintln!("  ✓ {}", summary.lines().next().unwrap_or("committed"));
    }

    if exit_code != 0 {
        std::process::exit(exit_code);
    }

    let _ = manifest;
    Ok(())
}

fn cmd_exec_prompt(mgr: &InstanceManager, instance: &str, query: &str) -> Result<()> {
    let _manifest = mgr
        .get(instance)?
        .ok_or_else(|| anyhow::anyhow!("No such instance: {}", instance))?;

    let inst_dir = mgr.base_dir().join(instance);

    // Read persona (try occupation/persona.md first, fall back to persona.md)
    let persona_path = agents::instance::resolve_path(&inst_dir, "occupation/persona.md", "persona.md");
    let persona = std::fs::read_to_string(persona_path).unwrap_or_default();

    // Read skill catalog from Agentfile
    let skill_info = if let Ok(spec) = AgentfileSpec::load(&inst_dir) {
        spec.format_help(instance)
    } else {
        String::new()
    };

    // Discover org members if this instance is a router
    let manifest = mgr.get(instance)?.unwrap();
    let org_members_info = if manifest.org_router.as_deref() == Some(instance) {
        // I am the router — list my org members
        if let Some(ref org) = manifest.org {
            let all = mgr.list()?;
            let members: Vec<String> = all.iter()
                .filter(|i| i.org.as_deref() == Some(org.as_str()) && i.name != instance)
                .map(|i| {
                    let gh = mgr.get(&i.name).ok().flatten()
                        .and_then(|m| m.github_repo)
                        .unwrap_or_default();
                    format!("  - {} (repo: {}, status: {})", i.name, gh, i.status)
                })
                .collect();
            if members.is_empty() { String::new() }
            else { format!("\n\n## Org Members (you are the router)\n{}", members.join("\n")) }
        } else { String::new() }
    } else { String::new() };

    // Compose prompt for Claude
    let prompt = format!(
        "You are an agent router. Respond ONLY with action lines. No explanation, no markdown, no conversation.\n\n\
         Actions (one per line):\n\
         EXEC: <skill_name> [args]\n\
         DISPATCH: <member_instance> <task description>\n\
         REPLY: <message>\n\n\
         Rules:\n\
         - skill_name = skill name only (e.g. 'cool', 'mail'), NOT instance name\n\
         - DISPATCH opens a GitHub issue on the member's repo\n\
         - Multiple actions allowed\n\
         - REPLY only if no action needed\n\n\
         ## Persona\n{}\n\n## Skills\n{}{}\n\n## Query\n{}",
        persona, skill_info, org_members_info, query
    );

    // Call claude -p
    let output = std::process::Command::new("claude")
        .arg("-p")
        .arg(&prompt)
        .output();

    let claude_response = match output {
        Ok(o) if o.status.success() => {
            String::from_utf8_lossy(&o.stdout).to_string()
        }
        Ok(o) => {
            // Try ollama as fallback
            if let Some(resp) = try_ollama(&prompt) {
                resp
            } else {
                bail!("claude -p failed: {}", String::from_utf8_lossy(&o.stderr));
            }
        }
        Err(_) => {
            // claude not found, try ollama
            if let Some(resp) = try_ollama(&prompt) {
                resp
            } else {
                bail!("No LLM available. Install Claude Code CLI (claude) or Ollama.");
            }
        }
    };

    mgr.append_log(instance, &format!("prompt: {}", query))?;

    // Log LLM's decision
    let action_lines: Vec<&str> = claude_response.lines()
        .filter(|l| {
            let t = l.trim();
            t.starts_with("EXEC:") || t.starts_with("DISPATCH:") || t.starts_with("REPLY:") || t.starts_with("NONE:")
        })
        .collect();
    if !action_lines.is_empty() {
        let _ = mgr.append_log(instance, &format!("prompt-plan: {}", action_lines.join(" | ")));
    }

    // Parse response — look for EXEC: lines
    let mut executed = false;
    let mut exec_outputs: Vec<String> = Vec::new();
    for line in claude_response.lines() {
        let line = line.trim();
        if let Some(cmd) = line.strip_prefix("EXEC:") {
            let cmd = cmd.trim();
            let parts: Vec<&str> = cmd.splitn(2, ' ').collect();
            let skill_name = parts[0];
            let skill_args = if parts.len() > 1 { parts[1] } else { "" };

            println!("[running {} {}]", skill_name, skill_args);

            // Execute the skill
            let scoped_env = load_scoped_env(&inst_dir, Some(skill_name))?;
            if let Some(local_script) = resolve_skill_script(&inst_dir, skill_name) {
                let (exit_code, stdout, stderr) =
                    exec_skill_script(&local_script, skill_args, &inst_dir, &scoped_env)?;
                if !stdout.is_empty() {
                    print!("{}", stdout);
                    exec_outputs.push(format!("Output of EXEC: {} {}:\n{}", skill_name, skill_args, stdout));
                }
                if !stderr.is_empty() {
                    eprint!("{}", stderr);
                }
                mgr.append_log(
                    instance,
                    &format!(
                        "prompt-exec: {} → {} (exit {})",
                        cmd,
                        if exit_code == 0 { "ok" } else { "fail" },
                        exit_code
                    ),
                )?;
            } else {
                println!("skill not found: {}", skill_name);
            }
            executed = true;
        } else if let Some(cmd) = line.strip_prefix("DISPATCH:") {
            let cmd = cmd.trim();
            let parts: Vec<&str> = cmd.splitn(2, ' ').collect();
            let target = parts[0];
            let task = if parts.len() > 1 { parts[1] } else { query };

            // Find target's github_repo
            let target_repo = mgr.get(target).ok().flatten()
                .and_then(|m| m.github_repo);

            if let Some(repo) = target_repo {
                eprintln!("  ✓ DISPATCH: {} → {}", target, task);
                let title = if task.len() > 60 { format!("{}...", &task[..57]) } else { task.to_string() };
                let body = format!("Dispatched by router `{}`.\n\n{}", instance, task);
                let r = std::process::Command::new("gh")
                    .args(["issue", "create", "--repo", &repo, "--title", &title, "--body", &body, "--label", "dispatch"])
                    .output();
                match r {
                    Ok(o) if o.status.success() => {
                        let url = String::from_utf8_lossy(&o.stdout);
                        eprintln!("  ✓ issue opened: {}", url.trim());
                        mgr.append_log(instance, &format!("dispatch: {} → {} ({})", target, task, url.trim()))?;
                    }
                    _ => eprintln!("  ✗ failed to open issue on {}", repo),
                }
            } else {
                eprintln!("  ✗ DISPATCH: {} has no github_repo", target);
            }
            executed = true;
        } else if let Some(explanation) = line.strip_prefix("REPLY:") {
            println!("{}", explanation.trim());
            executed = true;
        } else if let Some(explanation) = line.strip_prefix("NONE:") {
            println!("{}", explanation.trim());
            executed = true;
        }
    }

    // If no EXEC: or NONE: found, just print the raw response
    if !executed {
        print!("{}", claude_response);
    }

    // Two-step dispatch: if we EXEC'd something but no DISPATCH, feed results back to LLM
    if executed && !claude_response.contains("DISPATCH:") && !org_members_info.is_empty() && !exec_outputs.is_empty() {
        let _ = mgr.append_log(instance, &format!("prompt-step2: analyzing {} exec results for dispatch", exec_outputs.len()));
        let exec_results = exec_outputs.join("\n\n");
        let follow_up = format!(
            "You ran skills and got these results:\n\n{}\n\n\
             Based on these results, which tasks should be DISPATCH'd to org members?\n\
             Respond ONLY with DISPATCH: lines or REPLY: if nothing to dispatch.\n\n\
             {}\n\nOriginal query: {}",
            exec_results, org_members_info, query
        );
        {
            let follow_resp = std::process::Command::new("claude")
                .arg("-p")
                .arg(&follow_up)
                .output()
                .ok()
                .filter(|o| o.status.success())
                .map(|o| String::from_utf8_lossy(&o.stdout).to_string());

            if let Some(resp) = follow_resp {
                for line in resp.lines() {
                    let line = line.trim();
                    if let Some(cmd) = line.strip_prefix("DISPATCH:") {
                        let cmd = cmd.trim();
                        let parts: Vec<&str> = cmd.splitn(2, ' ').collect();
                        let target = parts[0];
                        let task = if parts.len() > 1 { parts[1] } else { query };
                        let target_repo = mgr.get(target).ok().flatten().and_then(|m| m.github_repo);
                        if let Some(repo) = target_repo {
                            eprintln!("  ✓ DISPATCH: {} → {}", target, task);
                            let title: String = task.chars().take(60).collect();
                            let body = format!("Dispatched by router `{}`.\n\n{}", instance, task);
                            let r = std::process::Command::new("gh")
                                .args(["issue", "create", "--repo", &repo, "--title", &title, "--body", &body, "--label", "dispatch"])
                                .output();
                            if let Ok(o) = r {
                                if o.status.success() {
                                    let url = String::from_utf8_lossy(&o.stdout);
                                    eprintln!("  ✓ issue opened: {}", url.trim());
                                    let _ = mgr.append_log(instance, &format!("dispatch: {} → {} ({})", target, task, url.trim()));
                                }
                            }
                        }
                    } else if let Some(msg) = line.strip_prefix("REPLY:") {
                        println!("{}", msg.trim());
                    }
                }
            }
        }
    }

    Ok(())
}

fn try_ollama(prompt: &str) -> Option<String> {
    let output = std::process::Command::new("ollama")
        .args(["run", "llama3.2:3b"])
        .arg(prompt)
        .output()
        .ok()?;

    if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        None
    }
}

fn cmd_stop(mgr: &InstanceManager, instance: &str) -> Result<()> {
    let _manifest = mgr
        .get(instance)?
        .ok_or_else(|| anyhow::anyhow!("No such instance: {}", instance))?;

    mgr.append_log(instance, "stopped")?;
    println!("{}", instance);
    Ok(())
}

fn cmd_rm(mgr: &InstanceManager, instance: &str, keep_volumes: bool) -> Result<()> {
    println!("── rm: {} ──", instance);
    if keep_volumes {
        println!("  ✓ memory backed up to .{}.memory.bak", instance);
    }
    if mgr.remove(instance, keep_volumes)? {
        println!("  ✓ instance removed");
    } else {
        println!("  ✗ instance not found");
        bail!("No such instance: {}", instance);
    }
    Ok(())
}

fn cmd_logs(mgr: &InstanceManager, instance: &str, tail: usize, follow: bool) -> Result<()> {
    let _ = mgr.get(instance)?
        .ok_or_else(|| anyhow::anyhow!("No such instance: {}", instance))?;

    let logs = mgr.read_logs(instance, tail)?;
    for line in &logs {
        println!("{}", line);
    }

    if follow {
        use std::io::{Read, Seek, SeekFrom};

        let log_path = mgr.log_path(instance);
        // Open or wait for the log file to appear
        let mut file = if log_path.exists() {
            std::fs::File::open(&log_path)?
        } else {
            // If no log file yet, wait for it to appear
            loop {
                if log_path.exists() {
                    break std::fs::File::open(&log_path)?;
                }
                std::thread::sleep(std::time::Duration::from_millis(500));
            }
        };

        // Seek to end so we only print new content
        file.seek(SeekFrom::End(0))?;

        let mut buf = String::new();
        loop {
            buf.clear();
            let n = file.read_to_string(&mut buf)?;
            if n > 0 {
                print!("{}", buf);
            }
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
    }

    Ok(())
}

fn cmd_inspect(mgr: &InstanceManager, instance: &str) -> Result<()> {
    let manifest = mgr
        .get(instance)?
        .ok_or_else(|| anyhow::anyhow!("No such instance: {}", instance))?;

    let inst_dir = mgr.base_dir().join(instance);

    // Output JSON like docker inspect
    let inspect = serde_json::json!({
        "Name": manifest.name,
        "Image": manifest.agent_type,
        "Created": manifest.created_at.to_rfc3339(),
        "State": {
            "Status": "running",
        },
        "Config": {
            "Email": manifest.email,
            "Role": manifest.role,
            "Domains": manifest.domains,
        },
        "Mounts": {
            "Memory": agents::instance::resolve_path(&inst_dir, "cognition/memory", "memory").display().to_string(),
            "Logs": agents::instance::resolve_path(&inst_dir, "cognition/logs", "logs").display().to_string(),
            "Skills": agents::instance::resolve_path(&inst_dir, "occupation/skills", "skills").display().to_string(),
            "Persona": agents::instance::resolve_path(&inst_dir, "occupation/persona.md", "persona.md").display().to_string(),
        },
        "Cron": manifest.cron.iter().map(|c| {
            serde_json::json!({
                "Schedule": c.schedule,
                "Skill": c.skill,
                "LastRun": c.last_run.map(|t| t.to_rfc3339()),
            })
        }).collect::<Vec<_>>(),
        "Env": {
            "Scoped": load_scoped_env(&inst_dir, None).map(|e| e.len()).unwrap_or(0),
            "AgentfilePresent": inst_dir.join("occupation/Agentfile.toml").exists() || inst_dir.join("Agentfile.toml").exists(),
        },
    });

    println!("{}", serde_json::to_string_pretty(&inspect)?);
    Ok(())
}

fn cmd_images() -> Result<()> {
    let types_dir = aide_home().join("types");
    let builds_dir = aide_home().join("builds");

    println!(
        "{:<24} {:<10} {:<10} {}",
        "REPOSITORY", "TAG", "SIZE", "BUILT"
    );
    println!("{}", "─".repeat(60));

    // List pulled types
    if types_dir.exists() {
        for user_entry in std::fs::read_dir(&types_dir)? {
            let user_entry = user_entry?;
            if !user_entry.file_type()?.is_dir() { continue; }
            let user = user_entry.file_name().to_string_lossy().to_string();

            for type_entry in std::fs::read_dir(user_entry.path())? {
                let type_entry = type_entry?;
                if !type_entry.file_type()?.is_dir() { continue; }
                let type_name = type_entry.file_name().to_string_lossy().to_string();

                let repo = format!("{}/{}", user, type_name);
                let mut tag = "latest".to_string();
                let mut size = "—".to_string();

                if let Ok(spec) = AgentfileSpec::load(&type_entry.path()) {
                    tag = spec.agent.version;
                }

                // Check for build archive
                let archive = builds_dir.join(format!("{}-{}.tar.gz", type_name, tag));
                if archive.exists() {
                    if let Ok(meta) = std::fs::metadata(&archive) {
                        size = format_size(meta.len());
                    }
                }

                println!("{:<24} {:<10} {:<10}", repo, tag, size);
            }
        }
    }

    Ok(())
}

fn cmd_info(config: &AideConfig, mgr: &InstanceManager) -> Result<()> {
    let instances = mgr.list()?;
    let running = instances.iter().filter(|i| i.status != instance::InstanceStatus::Stopped).count();

    println!("Agent Instances: {}", instances.len());
    println!("  Running: {}", running);
    println!("  Stopped: {}", instances.len() - running);
    println!("Agent Types: {}", config.agents.len());
    println!("Machines: {}", config.machines.len());
    println!("Data Directory: {}", config.aide.data_dir);

    // Vault status
    let vault_path = config.aide.vault_path.as_deref().unwrap_or("~/.aide/vault.age");
    let expanded = shellexpand::tilde(vault_path).to_string();
    if Path::new(&expanded).exists() {
        println!("Vault: {} (encrypted)", vault_path);
    } else {
        println!("Vault: not configured");
    }

    // Hubs
    let hubs = hub::load_hubs();
    if hubs.is_empty() {
        println!("Hubs: none configured");
    } else {
        println!("Hubs: {}", hubs.iter().map(|h| h.repo.as_str()).collect::<Vec<_>>().join(", "));
    }

    println!();
    println!("Instances:");
    for inst in &instances {
        println!("  {} [{}] cron:{} — {}", inst.name, inst.agent_type, inst.cron_count, inst.status);
    }

    Ok(())
}

fn cmd_cron_ls(mgr: &InstanceManager, instance: &str) -> Result<()> {
    let entries = mgr.cron_list(instance)?;
    if entries.is_empty() {
        println!("No cron entries for {}.", instance);
        return Ok(());
    }

    println!(
        "{:<20} {:<16} {}",
        "SCHEDULE", "SKILL", "LAST RUN"
    );
    println!("{}", "─".repeat(52));

    for entry in &entries {
        let last = entry
            .last_run
            .map(|t| t.format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_else(|| "never".to_string());
        println!("{:<20} {:<16} {}", entry.schedule, entry.skill, last);
    }
    Ok(())
}

// ─── Mount / Unmount ───

fn cmd_mount(mgr: &InstanceManager, instance: &str, target: &str) -> Result<()> {
    let _manifest = mgr
        .get(instance)?
        .ok_or_else(|| anyhow::anyhow!("No such instance: {}", instance))?;

    let instance_dir = mgr.base_dir().join(instance);
    agents::mount::mount(&instance_dir, instance, target)?;
    mgr.append_log(instance, &format!("mounted to {}", target))?;
    Ok(())
}

fn cmd_unmount(mgr: &InstanceManager, instance: &str, target: &str) -> Result<()> {
    let _manifest = mgr
        .get(instance)?
        .ok_or_else(|| anyhow::anyhow!("No such instance: {}", instance))?;

    agents::mount::unmount(instance, target)?;
    mgr.append_log(instance, &format!("unmounted from {}", target))?;
    Ok(())
}

// ─── Skill execution ───

/// Resolve the skill script path, checking .sh and .ts extensions.
fn resolve_skill_script(inst_dir: &Path, skill_name: &str) -> Option<PathBuf> {
    // Try occupation/skills/ first, then root skills/ for backward compat
    let skills_dirs = [
        inst_dir.join("occupation/skills"),
        inst_dir.join("skills"),
    ];
    for skills_dir in &skills_dirs {
        for ext in &["ts", "sh"] {
            let path = skills_dir.join(format!("{}.{}", skill_name, ext));
            if path.exists() {
                return Some(path);
            }
        }
    }
    None
}

/// Find or install bun runtime for .ts skills.
pub fn find_or_install_bun() -> Result<PathBuf> {
    // Check PATH
    if let Ok(output) = std::process::Command::new("which").arg("bun").output() {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return Ok(PathBuf::from(path));
            }
        }
    }

    // Check common locations
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let candidates = [
        format!("{}/.bun/bin/bun", home),
        "/usr/local/bin/bun".to_string(),
    ];
    for candidate in &candidates {
        let path = PathBuf::from(candidate);
        if path.exists() {
            return Ok(path);
        }
    }

    // Auto-install
    tracing::info!("bun not found, installing...");
    let install = std::process::Command::new("bash")
        .args(["-c", "curl -fsSL https://bun.sh/install | bash 2>&1"])
        .output()
        .context("failed to run bun installer")?;

    if !install.status.success() {
        bail!(
            "bun installation failed: {}",
            String::from_utf8_lossy(&install.stderr)
        );
    }

    // Should be at ~/.bun/bin/bun now
    let installed = PathBuf::from(format!("{}/.bun/bin/bun", home));
    if installed.exists() {
        tracing::info!("bun installed at {}", installed.display());
        Ok(installed)
    } else {
        bail!("bun installed but binary not found at {}", installed.display())
    }
}

/// Execute a local skill script with scoped env.
/// Supports .sh (bash) and .ts (bun) scripts.
fn exec_skill_script(script: &Path, args: &str, working_dir: &Path, env: &[(String, String)]) -> Result<(i32, String, String)> {
    let ext = script.extension().and_then(|e| e.to_str()).unwrap_or("sh");

    let mut cmd = if ext == "ts" {
        let bun = find_or_install_bun()?;
        let mut c = std::process::Command::new(bun);
        c.arg("run");
        c.arg(script);
        c
    } else {
        let mut c = std::process::Command::new("bash");
        c.arg(script);
        c
    };

    if !args.is_empty() {
        for arg in args.split_whitespace() {
            cmd.arg(arg);
        }
    }
    cmd.current_dir(working_dir);
    for (k, v) in env {
        cmd.env(k, v);
    }

    let output = cmd.output()
        .with_context(|| format!("failed to execute script: {}", script.display()))?;

    Ok((
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    ))
}

/// Execute a skill via wonskill CLI with scoped env
fn exec_wonskill(skill: &str, args: &str, working_dir: &Path, env: &[(String, String)]) -> Result<(i32, String, String)> {
    let wonskill = which_wonskill()?;

    let mut cmd = std::process::Command::new(&wonskill);
    cmd.arg(skill);
    if !args.is_empty() {
        for arg in args.split_whitespace() {
            cmd.arg(arg);
        }
    }
    cmd.current_dir(working_dir);
    cmd.env("AIDE_INSTANCE_DIR", working_dir);
    for (k, v) in env {
        cmd.env(k, v);
    }

    let output = cmd.output()
        .with_context(|| format!("failed to execute aide-skill {} {}", skill, args))?;

    Ok((
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    ))
}

/// Find aide-skill (or legacy wonskill) binary
fn which_wonskill() -> Result<PathBuf> {
    // Try aide-skill first (current name), then wonskill (legacy)
    for bin_name in &["aide-skill", "wonskill"] {
        if let Ok(output) = std::process::Command::new("which").arg(bin_name).output() {
            if output.status.success() {
                let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !path.is_empty() {
                    return Ok(PathBuf::from(path));
                }
            }
        }
    }

    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let candidates = [
        format!("{}/.nvm/versions/node/v22.14.0/bin/aide-skill", home),
        format!("{}/.nvm/versions/node/v22.14.0/bin/wonskill", home),
        format!("{}/bin/aide-skill", home),
        format!("{}/bin/wonskill", home),
        "/usr/local/bin/aide-skill".to_string(),
        "/usr/local/bin/wonskill".to_string(),
    ];

    for candidate in &candidates {
        let path = PathBuf::from(candidate);
        if path.exists() {
            return Ok(path);
        }
    }

    bail!("aide-skill not found. Install it or add it to PATH.")
}

// ─── Credential scoping (Docker secrets model) ───

/// Load env vars with three-tier scoping (Docker secrets model):
///   1. Per-skill env (skill.env in Agentfile) — most restrictive
///   2. Per-agent env ([env] in Agentfile) — default
///   3. Full vault — only if no Agentfile (legacy)
fn load_scoped_env(inst_dir: &Path, skill_name: Option<&str>) -> Result<Vec<(String, String)>> {
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

    let spec = AgentfileSpec::load(inst_dir)
        .unwrap_or_else(|_| return_empty_spec());

    // Tier 1: per-skill env (if skill has its own env list, use ONLY those)
    if let Some(sname) = skill_name {
        if let Some(skill_def) = spec.skills.get(sname) {
            if let Some(skill_env) = &skill_def.env {
                let allowed: std::collections::HashSet<String> = skill_env.iter().cloned().collect();
                return Ok(all_env.into_iter().filter(|(k, _)| allowed.contains(k)).collect());
            }
        }
    }

    // Tier 2: per-agent env ([env] section)
    let allowed: std::collections::HashSet<String> = match &spec.env {
        Some(env_section) => {
            let mut set = std::collections::HashSet::new();
            for k in &env_section.required { set.insert(k.clone()); }
            for k in &env_section.optional { set.insert(k.clone()); }
            set
        }
        None => return Ok(Vec::new()),
    };

    Ok(all_env.into_iter().filter(|(k, _)| allowed.contains(k)).collect())
}

fn return_empty_spec() -> AgentfileSpec {
    AgentfileSpec {
        agent: agents::agentfile::AgentMeta {
            name: String::new(),
            version: String::new(),
            description: None,
            author: None,
        },
        persona: None,
        skills: std::collections::HashMap::new(),
        knowledge: None,
        env: None,
        soul: None,
        expose: None,
        limits: None,
    }
}

fn load_vault_env() -> Result<Vec<(String, String)>> {
    // Try ~/.aide/vault.age first, then vault repo as fallback
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let default_path = aide_home().join("vault.age");
    let vault_repo_path = PathBuf::from(&home).join("claude_projects/aide-vault/vault.age");
    let vault_path = if default_path.exists() { default_path } else { vault_repo_path };
    if !vault_path.exists() { return Ok(Vec::new()); }
    let identity_path = aide_home().join("vault.key");
    if !identity_path.exists() { return Ok(Vec::new()); }

    let output = std::process::Command::new("age")
        .args(["-d", "-i"])
        .arg(&identity_path)
        .arg(&vault_path)
        .output()?;

    if !output.status.success() { return Ok(Vec::new()); }

    let content = String::from_utf8_lossy(&output.stdout);
    let mut vars = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') { continue; }
        let line = line.strip_prefix("export ").unwrap_or(line);
        if let Some((key, val)) = line.split_once('=') {
            let val = val.trim_matches('"').trim_matches('\'');
            vars.push((key.to_string(), val.to_string()));
        }
    }
    Ok(vars)
}

// ─── Vault commands ───

/// Fix vault key file permissions (chmod 600)
/// Read a line from stdin without echoing (for secret input).
fn read_secure_input() -> Result<String> {
    // Disable echo via stty
    #[cfg(unix)]
    let _ = std::process::Command::new("stty").arg("-echo").status();

    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;

    // Restore echo
    #[cfg(unix)]
    let _ = std::process::Command::new("stty").arg("echo").status();
    eprintln!(); // newline after hidden input

    Ok(input.trim().to_string())
}

fn fix_vault_key_permissions(key_path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if key_path.exists() {
            if let Ok(meta) = std::fs::metadata(key_path) {
                let perms = meta.permissions();
                if perms.mode() & 0o077 != 0 {
                    let mut new_perms = perms;
                    new_perms.set_mode(0o600);
                    let _ = std::fs::set_permissions(key_path, new_perms);
                }
            }
        }
    }
}

/// Rotate vault: decrypt with old key, generate new key, re-encrypt
async fn cmd_vault_rotate(v: &vault::Vault) -> Result<()> {
    // Decrypt with current key
    let plaintext = v.decrypt().await
        .context("cannot rotate: failed to decrypt current vault")?;

    // Backup old key
    let key_path = v.identity_path();
    let backup = key_path.with_extension("key.bak");
    std::fs::copy(&key_path, &backup)
        .context("failed to backup old key")?;
    println!("  old key backed up to {}", backup.display());

    // Remove old key so init() generates a new one
    std::fs::remove_file(&key_path)?;

    // Generate new key
    v.init().await?;
    fix_vault_key_permissions(&key_path);

    // Re-encrypt with new key
    v.encrypt(&plaintext).await?;

    println!("vault rotated: new key at {}", key_path.display());
    println!("  old key: {} (delete after verifying)", backup.display());

    // Verify by decrypting
    let verify = v.decrypt().await?;
    if verify != plaintext {
        bail!("CRITICAL: rotation verification failed! Restore from {}", backup.display());
    }
    println!("  verified: decrypt with new key OK");

    Ok(())
}

/// Show vault status with security audit
async fn cmd_vault_status(v: &vault::Vault) {
    let key_path = v.identity_path();

    // Show public key
    if let Ok(pubkey) = v.recipient().await {
        println!("pubkey:   {}", pubkey);
    }

    if key_path.exists() {
        println!("key:      {}", key_path.display());

        // Check permissions
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let key_meta = std::fs::metadata(&key_path).unwrap();
            let mode = key_meta.permissions().mode() & 0o777;
            if mode & 0o077 != 0 {
                println!("  WARNING: key permissions {:o} too open (should be 600)", mode);
            } else {
                println!("  permissions: {:o} OK", mode);
            }
        }
    } else {
        println!("key:      not found");
    }

    // List keys
    match v.list_keys().await {
        Ok(keys) => println!("secrets:  {} keys", keys.len()),
        Err(_) => println!("secrets:  (cannot decrypt)"),
    }

    // Check registry token
    let tokens_path = aide_home().join("registry-tokens.json");
    if tokens_path.exists() {
        println!("registry: {} (plaintext — use `vault set-token` to migrate)", tokens_path.display());
    }
}

/// Store registry token in vault instead of plaintext file
async fn cmd_vault_set_token(v: &vault::Vault, username: &str, token: &str) -> Result<()> {
    // Decrypt current vault
    let plaintext = v.decrypt().await
        .context("failed to decrypt vault")?;
    let mut content = String::from_utf8(plaintext)?;

    // Remove old REGISTRY_TOKEN lines
    let lines: Vec<&str> = content.lines()
        .filter(|l| !l.starts_with("REGISTRY_TOKEN=") && !l.starts_with("export REGISTRY_TOKEN=")
                  && !l.starts_with("REGISTRY_USERNAME=") && !l.starts_with("export REGISTRY_USERNAME="))
        .collect();
    content = lines.join("\n");
    if !content.ends_with('\n') { content.push('\n'); }

    // Add new token
    content.push_str(&format!("export REGISTRY_USERNAME='{}'\n", username));
    content.push_str(&format!("export REGISTRY_TOKEN='{}'\n", token));

    // Re-encrypt
    v.encrypt(content.as_bytes()).await?;

    // Also update the plaintext tokens file for the local registry server (backward compat)
    let tokens_path = aide_home().join("registry-tokens.json");
    let tokens_json = serde_json::json!([
        { "token": token, "username": username }
    ]);
    std::fs::write(&tokens_path, serde_json::to_string_pretty(&tokens_json)?)?;

    println!("token stored in vault for user '{}'", username);
    println!("  also updated {}", tokens_path.display());
    Ok(())
}

/// `aide vault set KEY=VALUE [KEY2=VALUE2 ...]`
async fn cmd_vault_set(v: &vault::Vault, pairs: &[String]) -> Result<()> {
    if pairs.is_empty() {
        bail!("Usage: aide vault set KEY=VALUE or aide vault set KEY (secure input)");
    }

    // Parse pairs — support both KEY=VALUE and KEY (prompt for value)
    let mut new_vars: Vec<(String, String)> = Vec::new();
    for pair in pairs {
        if let Some((key, val)) = pair.split_once('=') {
            new_vars.push((key.to_string(), val.to_string()));
        } else {
            // No '=' — secure input mode
            let key = pair.to_string();
            eprint!("Enter value for {}: ", key);
            let val = read_secure_input()?;
            if val.is_empty() {
                bail!("empty value for {}", key);
            }
            new_vars.push((key, val));
        }
    }

    // Init vault if needed
    v.init().await?;
    fix_vault_key_permissions(&v.identity_path());

    // Load existing or start fresh
    let mut content = match v.decrypt().await {
        Ok(data) => String::from_utf8(data)?,
        Err(_) => String::new(),
    };

    // Upsert each key
    for (key, val) in &new_vars {
        // Remove old line if exists
        let lines: Vec<&str> = content.lines()
            .filter(|l| {
                let l = l.strip_prefix("export ").unwrap_or(l);
                !l.starts_with(&format!("{}=", key))
            })
            .collect();
        content = lines.join("\n");
        if !content.ends_with('\n') && !content.is_empty() { content.push('\n'); }
        content.push_str(&format!("export {}='{}'\n", key, val));
    }

    v.encrypt(content.as_bytes()).await?;

    for (key, _) in &new_vars {
        println!("  set {}", key);
    }
    println!("{} secret(s) stored in vault", new_vars.len());
    Ok(())
}

/// Scan files for potential credential leaks before push
fn scan_for_leaks(dir: &Path) -> Result<Vec<String>> {
    // Simple prefix-based detection (no regex needed)
    let secret_prefixes = [
        "sk-ant-",     // Anthropic
        "sk-proj-",    // OpenAI
        "AKIA",        // AWS
        "ghp_",        // GitHub PAT
        "gho_",        // GitHub OAuth
        "eyJhbG",      // JWT
        "-----BEGIN",  // PEM keys
    ];

    let mut leaks = Vec::new();

    fn walk(dir: &Path, prefixes: &[&str], leaks: &mut Vec<String>) -> Result<()> {
        if !dir.is_dir() { return Ok(()); }
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                walk(&path, prefixes, leaks)?;
            } else {
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                if ["gz", "tar", "zip", "png", "jpg", "bin", "age"].contains(&ext) {
                    continue;
                }
                if let Ok(content) = std::fs::read_to_string(&path) {
                    for (line_num, line) in content.lines().enumerate() {
                        for prefix in prefixes {
                            if line.contains(prefix) {
                                leaks.push(format!(
                                    "  {}:{}: possible secret ({}...)",
                                    path.strip_prefix(dir).unwrap_or(&path).display(),
                                    line_num + 1,
                                    prefix
                                ));
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    walk(dir, &secret_prefixes, &mut leaks)?;
    Ok(leaks)
}

// ─── Dashboard ───

async fn cmd_dash(data_dir: &str, port: u16) -> Result<()> {
    println!("aide dashboard → http://localhost:{}", port);
    dashboard::serve(data_dir, port).await
}

// ─── Init / Lint ───

fn cmd_init(name: &str) -> Result<()> {
    let dir = Path::new(name);
    if dir.exists() {
        bail!("directory '{}' already exists", name);
    }
    println!("── init: {} ──", name);
    agents::scaffold::init_agent(name, dir)?;
    println!("  ✓ occupation/Agentfile.toml");
    println!("  ✓ occupation/persona.md");
    println!("  ✓ occupation/skills/hello.ts");
    println!("  ✓ occupation/knowledge/");
    println!("  ✓ cognition/memory/");
    println!("  ✓ cognition/logs/");
    println!("  ✓ .aideignore");
    println!("  ✓ README.md");
    println!();
    println!("configure:");
    println!("  occupation/Agentfile.toml  — skills, env, limits (timeout/retry), expose");
    println!("  occupation/persona.md      — agent personality");
    println!("  aide.toml [aide]           — daily_commit_hour (default: 3, local time)");
    println!();
    println!("next: edit occupation/Agentfile.toml, then `aide build {}/`", name);
    Ok(())
}

fn cmd_lint(dir: &Path) -> Result<()> {
    let result = agents::lint::lint_agent(dir)?;
    agents::lint::print_lint_result(&result);
    if !result.errors.is_empty() {
        std::process::exit(1);
    }
    Ok(())
}

// ─── Build / Push / Pull / Login / Search ───

fn aide_home() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".aide")
}

fn format_size(bytes: u64) -> String {
    if bytes < 1024 { return format!("{}B", bytes); }
    if bytes < 1024 * 1024 { return format!("{:.1}KB", bytes as f64 / 1024.0); }
    format!("{:.1}MB", bytes as f64 / (1024.0 * 1024.0))
}

fn cmd_build(dir: &Path) -> Result<()> {
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use sha2::{Digest, Sha256};

    let dir = std::fs::canonicalize(dir)
        .with_context(|| format!("directory not found: {}", dir.display()))?;

    let spec = AgentfileSpec::load(&dir)?;
    let warnings = spec.validate(&dir)?;
    for w in &warnings {
        println!("  warn: {}", w);
    }

    // Scan for credential leaks before building (only scan occupation/ if it exists)
    let scan_dir = AgentfileSpec::base_dir(&dir);
    let leaks = scan_for_leaks(&scan_dir)?;
    if !leaks.is_empty() {
        eprintln!("BLOCKED: potential secrets detected in agent files:");
        for leak in &leaks {
            eprintln!("  {}", leak);
        }
        bail!("Fix leaks before building. Never include API keys, tokens, or passwords in agent files.");
    }

    println!(
        "building {}: {}",
        spec.agent.name, spec.agent.version
    );

    let builds_dir = aide_home().join("builds");
    std::fs::create_dir_all(&builds_dir)?;

    let archive_name = spec.archive_name();
    let archive_path = builds_dir.join(&archive_name);

    let tar_file = std::fs::File::create(&archive_path)
        .with_context(|| format!("failed to create {}", archive_path.display()))?;
    let enc = GzEncoder::new(tar_file, Compression::default());
    let mut tar_builder = tar::Builder::new(enc);

    let files = spec.collect_files(&dir)?;
    for file_path in &files {
        if file_path.exists() {
            let rel = file_path.strip_prefix(&dir).unwrap_or(file_path);
            tar_builder
                .append_path_with_name(file_path, rel)
                .with_context(|| format!("failed to add {} to archive", file_path.display()))?;
        }
    }

    tar_builder.finish()?;
    drop(tar_builder);

    let archive_bytes = std::fs::read(&archive_path)?;
    let mut hasher = Sha256::new();
    hasher.update(&archive_bytes);
    let checksum = hex::encode(hasher.finalize());

    println!(
        "  sha256: {}  {:.1}KB",
        &checksum[..12],
        archive_bytes.len() as f64 / 1024.0
    );

    Ok(())
}

fn cmd_push(dir: &Path) -> Result<()> {
    let dir = std::fs::canonicalize(dir)
        .with_context(|| format!("directory not found: {}", dir.display()))?;

    let spec = AgentfileSpec::load(&dir)?;

    // Scan for credential leaks before push
    let leaks = scan_for_leaks(&dir)?;
    if !leaks.is_empty() {
        eprintln!("BLOCKED: potential secrets detected:");
        for leak in &leaks {
            eprintln!("  {}", leak);
        }
        bail!("Fix leaks before pushing.");
    }

    // Determine target hub (default hub)
    let hubs = hub::load_hubs();
    let target_hub = hubs.iter()
        .find(|h| h.default)
        .or(hubs.first())
        .ok_or_else(|| anyhow::anyhow!("no hubs configured. Run: aide hub add <owner/repo>"))?;

    println!("pushing {} → {}", spec.agent.name, target_hub.repo);

    hub::push_to_hub(&spec.agent.name, &dir, &target_hub.repo)?;

    println!("{}:{} pushed to {}", spec.agent.name, spec.agent.version, target_hub.repo);
    Ok(())
}

/// Pull latest changes for an existing git-backed instance.
fn cmd_pull_instance(inst_dir: &Path, name: &str) -> Result<()> {
    let output = std::process::Command::new("git")
        .args(["pull", "--rebase"])
        .current_dir(inst_dir)
        .output()?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let head = std::process::Command::new("git")
            .args(["rev-parse", "--short", "HEAD"])
            .current_dir(inst_dir)
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default();
        println!("pulled {} (HEAD: {})", name, head);
        if !stdout.trim().is_empty() && stdout.trim() != "Already up to date." {
            print!("{}", stdout);
        }
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("pull failed: {}", stderr);
    }
    Ok(())
}

fn cmd_pull(agent_ref: &str) -> Result<()> {
    // agent_ref can be:
    //   "agent-name"       → pull from default hub
    //   "user/agent-name"  → pull from hub owned by user (search all hubs)

    let (hub_repo, agent_name) = if agent_ref.contains('/') {
        let parts: Vec<&str> = agent_ref.splitn(2, '/').collect();
        // Check if user/agent matches a specific hub owner
        let hubs = hub::load_hubs();
        let hub = hubs.iter()
            .find(|h| h.repo.starts_with(parts[0]))
            .or(hubs.first())
            .ok_or_else(|| anyhow::anyhow!("no hubs configured. Run: aide hub add <owner/repo>"))?;
        (hub.repo.clone(), parts[1].to_string())
    } else {
        let hubs = hub::load_hubs();
        let hub = hubs.iter()
            .find(|h| h.default)
            .or(hubs.first())
            .ok_or_else(|| anyhow::anyhow!("no hubs configured. Run: aide hub add <owner/repo>"))?;
        (hub.repo.clone(), agent_ref.to_string())
    };

    println!("pulling {} from {}...", agent_name, hub_repo);

    let types_dir = hub::pull_from_hub(&agent_name, &hub_repo)?;

    if let Ok(spec) = AgentfileSpec::load(&types_dir) {
        println!("{}:{}", agent_name, spec.agent.version);
    } else {
        println!("{}", agent_name);
    }

    Ok(())
}

async fn cmd_login() -> Result<()> {
    let client_id = "PLACEHOLDER_CLIENT_ID";

    println!("aide login — authenticating via GitHub");
    println!();

    let client = reqwest::Client::new();

    let resp = client
        .post("https://github.com/login/device/code")
        .header("Accept", "application/json")
        .form(&[("client_id", client_id), ("scope", "read:user")])
        .send()
        .await;

    let device_resp: serde_json::Value = match resp {
        Ok(r) if r.status().is_success() => r.json().await?,
        Ok(r) => {
            let status = r.status();
            let body = r.text().await.unwrap_or_default();
            bail!("device code request failed ({}): {}", status, body);
        }
        Err(e) => {
            bail!(
                "failed to reach GitHub: {}\n\nManual auth: create ~/.aide/auth.json with:\n{{\"token\": \"...\", \"username\": \"...\"}}",
                e
            );
        }
    };

    let device_code = device_resp["device_code"].as_str()
        .ok_or_else(|| anyhow::anyhow!("missing device_code"))?;
    let user_code = device_resp["user_code"].as_str()
        .ok_or_else(|| anyhow::anyhow!("missing user_code"))?;
    let verification_uri = device_resp["verification_uri"].as_str()
        .unwrap_or("https://github.com/login/device");
    let interval = device_resp["interval"].as_u64().unwrap_or(5);

    println!("Open: {}", verification_uri);
    println!("Code: {}", user_code);
    println!();
    println!("Waiting for authorization...");

    loop {
        tokio::time::sleep(std::time::Duration::from_secs(interval)).await;

        let token_resp = client
            .post("https://github.com/login/oauth/access_token")
            .header("Accept", "application/json")
            .form(&[
                ("client_id", client_id),
                ("device_code", device_code),
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ])
            .send().await?.json::<serde_json::Value>().await?;

        if let Some(error) = token_resp.get("error").and_then(|v| v.as_str()) {
            match error {
                "authorization_pending" => continue,
                "slow_down" => { tokio::time::sleep(std::time::Duration::from_secs(5)).await; continue; }
                "expired_token" => bail!("device code expired — run `aide login` again"),
                "access_denied" => bail!("authorization denied"),
                _ => bail!("OAuth error: {}", error),
            }
        }

        if let Some(access_token) = token_resp.get("access_token").and_then(|v| v.as_str()) {
            let user_resp = client
                .get("https://api.github.com/user")
                .header("Authorization", format!("Bearer {}", access_token))
                .header("User-Agent", "aide")
                .send().await?.json::<serde_json::Value>().await?;

            let username = user_resp["login"].as_str().unwrap_or("unknown").to_string();
            let email = user_resp["email"].as_str().unwrap_or("").to_string();

            let auth = serde_json::json!({
                "token": access_token,
                "username": username,
                "email": email,
                "provider": "github",
            });

            let auth_path = aide_home().join("auth.json");
            std::fs::create_dir_all(aide_home())?;
            std::fs::write(&auth_path, serde_json::to_string_pretty(&auth)?)?;

            // Analytics: register login (fire and forget)
            let analytics_payload = serde_json::json!({
                "event": "login",
                "username": username,
            });
            let _ = client
                .post("https://aide-analytics.aide.sh/v1/events")
                .json(&analytics_payload)
                .send()
                .await;

            println!("Login Succeeded ({})", username);
            return Ok(());
        }
    }
}

fn cmd_search(query: &str) -> Result<()> {
    let hubs = hub::load_hubs();
    if hubs.is_empty() {
        bail!("no hubs configured. Run: aide hub add <owner/repo>");
    }

    let mut any_results = false;

    for h in &hubs {
        match hub::search_hub(query, &h.repo) {
            Ok(results) => {
                if results.is_empty() {
                    continue;
                }
                if !any_results {
                    println!(
                        "{:<24} {:<10} {:<12} {}",
                        "NAME", "VERSION", "AUTHOR", "DESCRIPTION"
                    );
                    println!("{}", "\u{2500}".repeat(72));
                }
                for agent in &results {
                    println!(
                        "{:<24} {:<10} {:<12} {}",
                        agent.name,
                        agent.version,
                        agent.author,
                        agent.description,
                    );
                }
                any_results = true;
            }
            Err(e) => {
                eprintln!("warning: failed to search hub '{}': {}", h.name, e);
            }
        }
    }

    if !any_results {
        println!("no results for '{}'", query);
    }

    Ok(())
}

fn cmd_hub(action: &HubAction) -> Result<()> {
    match action {
        HubAction::Init { name, private } => {
            let visibility = if *private { "private" } else { "public" };
            println!("initializing hub repo '{}'...", name);
            hub::init_hub(name, visibility)?;
            println!("hub '{}' created ({})", name, visibility);
            println!();
            println!("add it as a source:");
            println!("  aide hub add {}", name);
        }
        HubAction::Add { repo } => {
            hub::add_hub(repo)?;
            println!("hub added: {}", repo);
        }
        HubAction::Ls => {
            let hubs = hub::load_hubs();
            if hubs.is_empty() {
                println!("no hubs configured. Run: aide hub add <owner/repo>");
                return Ok(());
            }
            println!("{:<16} {:<30} {}", "NAME", "REPO", "DEFAULT");
            println!("{}", "\u{2500}".repeat(52));
            for h in &hubs {
                let default_marker = if h.default { "*" } else { "" };
                println!("{:<16} {:<30} {}", h.name, h.repo, default_marker);
            }
        }
        HubAction::Rm { name } => {
            if hub::remove_hub(name)? {
                println!("hub '{}' removed", name);
            } else {
                println!("hub '{}' not found", name);
            }
        }
    }
    Ok(())
}

fn cmd_commit(data_dir: &str, instance: &str, message: &str) -> Result<()> {
    let mgr = InstanceManager::new(data_dir);
    let _manifest = mgr.get(instance)?
        .ok_or_else(|| anyhow::anyhow!("No such instance: {}", instance))?;

    println!("── commit: {} ──", instance);
    let inst_dir = mgr.base_dir().join(instance);

    if !inst_dir.join(".git").exists() {
        println!("  ✗ not a git repo");
        bail!(
            "Instance '{}' is not a git repo.\nRun: aide deploy --github {}",
            instance, instance
        );
    }
    println!("  ✓ git repo");

    // In-place commit + push + sanity check
    match agents::commit::auto_commit_instance(&inst_dir, message) {
        Some(summary) => {
            // Parse summary for checklist
            for line in summary.lines() {
                if line.starts_with("committed:") || line.starts_with("pushed:") {
                    println!("  ✓ {}", line);
                } else if !line.is_empty() {
                    println!("  {}", line);
                }
            }
            mgr.append_log(instance, &format!("commit: {}", message))?;
        }
        None => {
            println!("  ✓ nothing to commit (clean)");
        }
    }

    Ok(())
}

fn cmd_clean(include_vault: bool) -> Result<()> {
    let aide_dir = aide_home();
    if !aide_dir.exists() {
        println!("nothing to clean — {} does not exist", aide_dir.display());
        return Ok(());
    }

    let mut removed = Vec::new();

    // Always remove these
    let dirs_to_remove = ["instances", "builds", "types"];
    for dir_name in &dirs_to_remove {
        let path = aide_dir.join(dir_name);
        if path.exists() {
            std::fs::remove_dir_all(&path)?;
            removed.push(format!("  {}/", path.display()));
        }
    }

    let files_to_remove = ["auth.json", "registry-tokens.json"];
    for file_name in &files_to_remove {
        let path = aide_dir.join(file_name);
        if path.exists() {
            std::fs::remove_file(&path)?;
            removed.push(format!("  {}", path.display()));
        }
    }

    // Vault keys only with --include-vault
    if include_vault {
        let vault_files = ["vault.key", "vault.age"];
        for file_name in &vault_files {
            let path = aide_dir.join(file_name);
            if path.exists() {
                std::fs::remove_file(&path)?;
                removed.push(format!("  {} (vault)", path.display()));
            }
        }
    }

    if removed.is_empty() {
        println!("nothing to clean");
    } else {
        println!("removed:");
        for item in &removed {
            println!("{}", item);
        }
        println!();
        if !include_vault {
            let vault_key = aide_dir.join("vault.key");
            if vault_key.exists() {
                println!("vault keys preserved. Use --include-vault to remove them too.");
            }
        } else {
            println!("WARNING: vault keys removed. You will need to re-import secrets.");
        }
    }

    Ok(())
}

fn cmd_whoami() -> Result<()> {
    let auth_path = aide_home().join("auth.json");
    if !auth_path.exists() {
        println!("Not logged in. Run: aide login");
        return Ok(());
    }
    let content = std::fs::read_to_string(&auth_path)?;
    let auth: serde_json::Value = serde_json::from_str(&content)?;

    println!("Username: {}", auth["username"].as_str().unwrap_or("?"));
    println!("Email:    {}", auth["email"].as_str().unwrap_or("?"));
    println!("Provider: {}", auth["provider"].as_str().unwrap_or("?"));
    println!("Plan:     free");
    Ok(())
}

fn cmd_cost(data_dir: &str) -> Result<()> {
    let mgr = InstanceManager::new(data_dir);
    let instances = mgr.list()?;

    if instances.is_empty() {
        println!("No instances. Nothing to report.");
        return Ok(());
    }

    println!("{:<25} {:<10} {:<10} {:<10}", "INSTANCE", "EXECS", "SUCCESS", "FAIL");
    println!("{}", "\u{2500}".repeat(55));

    for inst in &instances {
        let logs = mgr.read_logs(&inst.name, 10000).unwrap_or_default();
        let mut total = 0u32;
        let mut success = 0u32;
        let mut fail = 0u32;

        for line in &logs {
            if line.contains("exec-result:") || line.contains("mcp-exec-result:") || line.contains("cron-result:") {
                total += 1;
                if line.contains("\u{2192} ok") || line.contains("-> ok") {
                    success += 1;
                } else {
                    fail += 1;
                }
            }
        }

        if total > 0 {
            println!("{:<25} {:<10} {:<10} {:<10}", inst.name, total, success, fail);
        }
    }

    Ok(())
}


fn cmd_deploy_github(data_dir: &str, instance: &str, private: bool) -> Result<()> {
    let mgr = InstanceManager::new(data_dir);
    let _manifest = mgr.get(instance)?
        .ok_or_else(|| anyhow::anyhow!("No such instance: {}", instance))?;

    let inst_dir = mgr.base_dir().join(instance);

    // Read auth for username
    let auth_path = aide_home().join("auth.json");
    let username = if let Ok(content) = std::fs::read_to_string(&auth_path) {
        serde_json::from_str::<serde_json::Value>(&content)
            .ok()
            .and_then(|v| v["username"].as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| "user".to_string())
    } else {
        bail!("Not logged in. Run: aide login");
    };

    // Derive repo name from instance: school.ydwu → aide-school
    let agent_name = instance.split('.').next().unwrap_or(instance);
    let repo_name = format!("aide-{}", agent_name);
    let visibility = if private { "--private" } else { "--public" };
    let github_repo_ref = format!("{}/{}", username, repo_name);

    println!("── deploy: {} → github.com/{} ──", instance, github_repo_ref);

    // 1. Create repo via gh CLI
    let create_output = std::process::Command::new("gh")
        .args(["repo", "create", &github_repo_ref, visibility, "--confirm"])
        .output()?;

    if !create_output.status.success() {
        let stderr = String::from_utf8_lossy(&create_output.stderr);
        if !stderr.contains("already exists") {
            println!("  ✗ repo creation failed: {}", stderr.trim());
            bail!("failed to create repo: {}", stderr);
        }
        println!("  ✓ repo exists (reusing)");
    } else {
        println!("  ✓ repo created ({})", if private { "private" } else { "public" });
    }

    // 2. Git init in-place (instance dir = git repo)
    let git = |args: &[&str]| -> Result<bool> {
        let output = std::process::Command::new("git")
            .args(args)
            .current_dir(&inst_dir)
            .output()?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if !stderr.contains("already exists") && !stderr.contains("nothing to commit") {
                tracing::warn!("git {:?}: {}", args, stderr);
            }
        }
        Ok(output.status.success())
    };

    let is_git_repo = inst_dir.join(".git").exists();

    if !is_git_repo {
        // Write .gitignore before init
        write_instance_gitignore(&inst_dir)?;

        // Ensure .gitkeep in empty dirs
        std::fs::create_dir_all(inst_dir.join("cognition/memory"))?;
        if std::fs::read_dir(inst_dir.join("cognition/memory"))?.count() == 0 {
            std::fs::write(inst_dir.join("cognition/memory/.gitkeep"), "")?;
        }
        std::fs::create_dir_all(inst_dir.join("occupation/knowledge"))?;
        if !inst_dir.join("occupation/knowledge/.gitkeep").exists()
            && std::fs::read_dir(inst_dir.join("occupation/knowledge")).map(|d| d.count()).unwrap_or(0) == 0
        {
            std::fs::write(inst_dir.join("occupation/knowledge/.gitkeep"), "")?;
        }

        // Create README
        let readme = if let Ok(spec) = AgentfileSpec::load(&inst_dir) {
            let desc = spec.agent.description.as_deref().unwrap_or("An aide agent");
            let persona_note = if spec.persona.is_some() {
                "\n\nSee [occupation/persona.md](occupation/persona.md) for agent personality and behavior.\n"
            } else {
                "\n"
            };
            format!("# {}\n\n{}{}\nPowered by [aide.sh](https://aide.sh)\n",
                agent_name, desc, persona_note)
        } else {
            format!("# {}\n\nAn aide agent.\n", agent_name)
        };
        std::fs::write(inst_dir.join("README.md"), readme)?;

        step!(git(&["init"])?, "git init");
        step!(git(&["remote", "add", "origin", &format!("git@github.com:{}.git", github_repo_ref)])?, "remote origin set");
    } else {
        println!("  ✓ git repo exists");
        // Ensure remote is set even if git already initialized
        let has_remote = std::process::Command::new("git")
            .args(["remote", "get-url", "origin"])
            .current_dir(&inst_dir)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if !has_remote {
            step!(git(&["remote", "add", "origin", &format!("git@github.com:{}.git", github_repo_ref)])?, "remote origin set");
        } else {
            println!("  ✓ remote origin configured");
        }
    }

    git(&["add", "-A"])?;

    // Check if there are changes to commit
    let has_changes = !git(&["diff", "--cached", "--quiet"])?;
    if has_changes {
        step!(git(&["commit", "-m", &format!("deploy {} agent", agent_name)])?, "committed");
    } else {
        println!("  ✓ nothing to commit (clean)");
    }

    git(&["branch", "-M", "main"])?;

    // Try normal push first. If repo has existing commits, pull --rebase then push.
    let pushed = if !git(&["push", "-u", "origin", "main"])? {
        git(&["pull", "--rebase", "origin", "main"])?;
        git(&["push", "-u", "origin", "main"])?
    } else {
        true
    };

    if !pushed {
        println!("  ✗ push failed — check SSH keys and repo permissions");
        bail!("push failed — check SSH keys and repo permissions, or resolve conflicts manually");
    }
    println!("  ✓ pushed to origin/main");

    // Write github_repo back to instance.toml
    if let Ok(Some(mut manifest)) = mgr.get(instance) {
        manifest.github_repo = Some(github_repo_ref.clone());
        let inst_path = mgr.base_dir().join(instance);
        let manifest_path = agents::instance::resolve_path(&inst_path, "cognition/instance.toml", "instance.toml");
        let content = toml::to_string_pretty(&manifest)?;
        std::fs::write(&manifest_path, content)?;
    }
    println!("  ✓ github_repo saved to instance.toml");

    // Sanity check
    let local_head = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(&inst_dir)
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    let short = &local_head[..7.min(local_head.len())];

    println!("  ✓ HEAD: {} — https://github.com/{}", short, github_repo_ref);
    mgr.append_log(instance, &format!("deploy-github: {} ({})", github_repo_ref, short))?;

    Ok(())
}

// ─── Git-native instance helpers ─────────────────────────────────

/// Auto-commit and push an instance directory if it's a git repo.
/// Returns a summary string on success, or None if no changes / not a git repo.
/// Fire-and-forget: never panics, never fails the caller.
// auto_commit_instance moved to agents::commit

/// Write the standard .gitignore for an aide instance repo.
fn write_instance_gitignore(inst_dir: &Path) -> Result<()> {
    std::fs::write(inst_dir.join(".gitignore"), "cognition/logs/\n*.log\n.DS_Store\n")?;
    Ok(())
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}
