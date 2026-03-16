#![allow(dead_code)]

mod agents;
mod config;
mod daemon;
mod dispatch;
mod email;
mod mcp;
mod sync;
mod vault;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};
use tracing_subscriber::EnvFilter;

use agents::agentfile::AgentfileSpec;
use agents::instance::{self, InstanceManager};
use config::AideConfig;

#[derive(Parser)]
#[command(name = "aide.sh", about = "Docker for AI agents — aide.sh", version)]
struct Cli {
    /// Path to aide.toml config file
    #[arg(short, long, default_value = "aide.toml")]
    config: PathBuf,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    // ─── Container lifecycle (Docker-style) ───

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

    // ─── Image management (Docker-style) ───

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

    // ─── System ───

    /// Display system-wide information
    Info,
    /// Start the aide daemon
    Up,
    /// Stop the aide daemon
    Down,

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
        Command::Push { image } => return cmd_push(image).await,
        Command::Pull { image } => {
            let (agent_ref, version) = parse_image_ref(image);
            return cmd_pull(&agent_ref, &version).await;
        }
        Command::Login => return cmd_login().await,
        Command::Search { query } => return cmd_search(query).await,
        Command::Images => return cmd_images(),
        Command::Init { name } => return cmd_init(name),
        Command::Lint { path } => return cmd_lint(path),
        Command::Mcp => {
            let config = AideConfig::load(&cli.config).unwrap_or_else(|_| AideConfig::default());
            return mcp::run_mcp_server(&config.aide.data_dir);
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
        Command::Exec { instance, command, interactive, tty } => {
            let skill = command.join(" ");
            cmd_exec(&mgr, &instance, &skill, interactive || tty)?;
        }
        Command::Call { instance, skill } => {
            cmd_exec(&mgr, &instance, &skill.join(" "), false)?;
        }
        Command::Ps { all: _ } => {
            cmd_ps(&mgr)?;
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
        Command::Up => {
            let d = daemon::Daemon::new(config);
            d.run().await?;
        }
        Command::Down => {
            println!("aide daemon stopping...");
            println!("(not yet implemented — kill the process manually)");
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
            let vault_path = config
                .aide
                .vault_path
                .as_ref()
                .map(|p| shellexpand::tilde(p).to_string())
                .unwrap_or_else(|| "~/.aide/vault.age".to_string());
            let targets = config
                .sync
                .vault
                .as_ref()
                .map(|v| v.targets.clone())
                .unwrap_or_default();
            let v = vault::Vault::new(PathBuf::from(&vault_path), targets);

            match action {
                VaultAction::Import { path } => {
                    v.import_env(&path).await?;
                    fix_vault_key_permissions(&v.identity_path());
                    println!("imported {} → {}", path.display(), vault_path);
                }
                VaultAction::Set { pairs } => {
                    cmd_vault_set(&v, &pairs).await?;
                }
                VaultAction::Rotate => {
                    cmd_vault_rotate(&v, &vault_path).await?;
                }
                VaultAction::Status => {
                    cmd_vault_status(&vault_path, &v);
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
        | Command::Search { .. }
        | Command::Init { .. }
        | Command::Lint { .. }
        | Command::Mcp => unreachable!(),
    }

    Ok(())
}

// ─── Parse image ref: "user/type:version" → ("user/type", "version") ───

fn parse_image_ref(image: &str) -> (String, String) {
    if let Some((ref_part, version)) = image.rsplit_once(':') {
        (ref_part.to_string(), version.to_string())
    } else {
        (image.to_string(), "latest".to_string())
    }
}

// ─── Command implementations ───

fn cmd_ps(mgr: &InstanceManager) -> Result<()> {
    let instances = mgr.list()?;
    if instances.is_empty() {
        println!("No agent instances. Use `aide.sh run <image>` to create one.");
        return Ok(());
    }

    println!(
        "{:<20} {:<12} {:<8} {:<6} {}",
        "INSTANCE", "IMAGE", "STATUS", "CRON", "LAST ACTIVITY"
    );
    println!("{}", "─".repeat(76));

    for inst in &instances {
        let last = inst.last_activity.as_deref().unwrap_or("—");
        println!(
            "{:<20} {:<12} {:<8} {:<6} {}",
            inst.name,
            inst.agent_type,
            inst.status,
            inst.cron_count,
            last
        );
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
            "image '{}' not found. Available: {}\nUse `aide.sh pull <user>/{}` to fetch from registry.",
            image,
            available.join(", "),
            image
        )
    })?;

    let instance_name = match name {
        Some(n) => n.to_string(),
        None => instance::default_instance_name(image),
    };

    let manifest = mgr.spawn(image, &instance_name, def)?;
    mgr.append_log(&instance_name, &format!("created from image '{}'", image))?;

    println!("{}", manifest.name);
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
            "image '{}' not found locally. Run `aide.sh pull {}` first.",
            image, image
        );
    }

    let spec = AgentfileSpec::load(&types_dir)?;

    let instance_name = match name {
        Some(n) => n.to_string(),
        None => instance::default_instance_name(agent_type),
    };

    let def = config::AgentDef {
        email: format!("{}@aide.sh", spec.agent.name),
        role: spec.agent.description.clone().unwrap_or_default(),
        domains: Vec::new(),
        persona_path: spec.persona.as_ref().map(|p| {
            types_dir.join(&p.file).to_string_lossy().to_string()
        }),
    };

    let manifest = mgr.spawn(agent_type, &instance_name, &def)?;
    mgr.append_log(&instance_name, &format!("created from image '{}'", image))?;

    // Copy Agentfile.toml into instance (image manifest travels with container)
    let inst_dir = mgr.base_dir().join(&instance_name);
    let agentfile_src = types_dir.join("Agentfile.toml");
    if agentfile_src.exists() {
        std::fs::copy(&agentfile_src, inst_dir.join("Agentfile.toml"))?;
    }

    // Copy skill files
    let skills_dir = inst_dir.join("skills");
    std::fs::create_dir_all(&skills_dir)?;

    for (skill_name, skill_def) in &spec.skills {
        if let Some(script) = &skill_def.script {
            let src = types_dir.join(script);
            if src.exists() {
                let dst = skills_dir.join(
                    Path::new(script)
                        .file_name()
                        .unwrap_or(std::ffi::OsStr::new(skill_name)),
                );
                std::fs::copy(&src, &dst)?;
            }
        }
        if let Some(prompt) = &skill_def.prompt {
            let src = types_dir.join(prompt);
            if src.exists() {
                let dst = skills_dir.join(
                    Path::new(prompt)
                        .file_name()
                        .unwrap_or(std::ffi::OsStr::new(skill_name)),
                );
                std::fs::copy(&src, &dst)?;
            }
        }
        if let Some(schedule) = &skill_def.schedule {
            mgr.cron_add(&instance_name, schedule, skill_name)?;
        }
    }

    // Copy seed data
    if let Some(seed) = &spec.seed {
        let seed_src = types_dir.join(&seed.dir);
        if seed_src.exists() {
            let seed_dst = inst_dir.join("seed");
            copy_dir_recursive(&seed_src, &seed_dst)?;
        }
    }

    println!("{}", manifest.name);
    Ok(())
}

fn cmd_exec(mgr: &InstanceManager, instance: &str, skill: &str, _interactive: bool) -> Result<()> {
    let manifest = mgr
        .get(instance)?
        .ok_or_else(|| anyhow::anyhow!("No such instance: {}", instance))?;

    // Handle --help: show available skills from Agentfile
    if skill == "--help" || skill == "-h" || skill.is_empty() {
        let inst_dir = mgr.base_dir().join(instance);
        if let Ok(spec) = AgentfileSpec::load(&inst_dir) {
            print!("{}", spec.format_help(instance));
        } else {
            println!("{} (no Agentfile.toml — skill discovery unavailable)", instance);
            println!("\nUsage: aide.sh exec {} <skill> [args...]", instance);
        }
        return Ok(());
    }

    // Parse skill input
    let parts: Vec<&str> = skill.splitn(2, ' ').collect();
    let skill_name = parts[0];
    let skill_args = if parts.len() > 1 { parts[1] } else { "" };

    mgr.append_log(instance, &format!("exec: {}", skill))?;

    // Load scoped env (Docker secrets model — per-skill > per-agent > vault)
    let inst_dir = mgr.base_dir().join(instance);
    let scoped_env = load_scoped_env(&inst_dir, Some(skill_name))?;

    // Resolve and dispatch
    let local_script = inst_dir.join("skills").join(format!("{}.sh", skill_name));

    let (exit_code, stdout, stderr) = if local_script.exists() {
        exec_skill_script(&local_script, skill_args, &inst_dir, &scoped_env)?
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
    mgr.append_log(
        instance,
        &format!("exec-result: {} → {} (exit {})", skill, status_msg, exit_code),
    )?;

    if exit_code != 0 {
        // Docker exec returns the exit code of the executed command
        std::process::exit(exit_code);
    }

    let _ = manifest; // used for future interactive mode
    Ok(())
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
    if mgr.remove(instance, keep_volumes)? {
        println!("{}", instance);
    } else {
        bail!("No such instance: {}", instance);
    }
    Ok(())
}

fn cmd_logs(mgr: &InstanceManager, instance: &str, tail: usize, _follow: bool) -> Result<()> {
    let _ = mgr.get(instance)?
        .ok_or_else(|| anyhow::anyhow!("No such instance: {}", instance))?;

    let logs = mgr.read_logs(instance, tail)?;
    if logs.is_empty() {
        return Ok(());
    }
    for line in &logs {
        println!("{}", line);
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
            "Memory": inst_dir.join("memory").display().to_string(),
            "Logs": inst_dir.join("logs").display().to_string(),
            "Skills": inst_dir.join("skills").display().to_string(),
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
            "AgentfilePresent": inst_dir.join("Agentfile.toml").exists(),
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
    let running = instances.iter().filter(|i| i.status == instance::InstanceStatus::Active).count();

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

    // Registry
    println!("Registry: https://hub.aide.sh");

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

/// Execute a local skill script with scoped env
fn exec_skill_script(script: &Path, args: &str, working_dir: &Path, env: &[(String, String)]) -> Result<(i32, String, String)> {
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
    for (k, v) in env {
        cmd.env(k, v);
    }

    let output = cmd.output()
        .with_context(|| format!("failed to execute wonskill {} {}", skill, args))?;

    Ok((
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    ))
}

/// Find wonskill binary
fn which_wonskill() -> Result<PathBuf> {
    if let Ok(output) = std::process::Command::new("which").arg("wonskill").output() {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return Ok(PathBuf::from(path));
            }
        }
    }

    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let candidates = [
        format!("{}/.nvm/versions/node/v22.14.0/bin/wonskill", home), // common nvm path
        format!("{}/bin/wonskill", home),
        "/usr/local/bin/wonskill".to_string(),
    ];

    for candidate in &candidates {
        let path = PathBuf::from(candidate);
        if path.exists() {
            return Ok(path);
        }
    }

    bail!("wonskill not found. Install it or add it to PATH.")
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

    let agentfile = inst_dir.join("Agentfile.toml");
    if !agentfile.exists() {
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
        seed: None,
        env: None,
        soul: None,
    }
}

fn load_vault_env() -> Result<Vec<(String, String)>> {
    let vault_path = aide_home().join("vault.age");
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
async fn cmd_vault_rotate(v: &vault::Vault, _vault_path: &str) -> Result<()> {
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
fn cmd_vault_status(vault_path: &str, v: &vault::Vault) {
    let path = std::path::Path::new(vault_path);
    if path.exists() {
        let meta = std::fs::metadata(path).unwrap();
        println!("vault:    {} ({} bytes)", vault_path, meta.len());
    } else {
        println!("vault:    {} (not found)", vault_path);
        return;
    }

    let key_path = v.identity_path();
    if key_path.exists() {
        let key_meta = std::fs::metadata(&key_path).unwrap();
        println!("key:      {}", key_path.display());

        // Check permissions
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = key_meta.permissions().mode() & 0o777;
            if mode & 0o077 != 0 {
                println!("  WARNING: key permissions {:o} too open (should be 600)", mode);
                println!("  fix: chmod 600 {}", key_path.display());
            } else {
                println!("  permissions: {:o} OK", mode);
            }
        }

        // Check if key and vault are in same directory (warning)
        if key_path.parent() == path.parent() {
            println!("  NOTE: key and vault in same directory");
        }
    } else {
        println!("key:      not found");
    }

    // Count env vars
    if let Ok(vars) = load_vault_env() {
        println!("env vars: {}", vars.len());
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

/// `aide.sh vault set KEY=VALUE [KEY2=VALUE2 ...]`
async fn cmd_vault_set(v: &vault::Vault, pairs: &[String]) -> Result<()> {
    if pairs.is_empty() {
        bail!("Usage: aide.sh vault set KEY=VALUE [KEY2=VALUE2 ...]");
    }

    // Parse pairs
    let mut new_vars: Vec<(String, String)> = Vec::new();
    for pair in pairs {
        let (key, val) = pair.split_once('=')
            .ok_or_else(|| anyhow::anyhow!("invalid format '{}' — expected KEY=VALUE", pair))?;
        new_vars.push((key.to_string(), val.to_string()));
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

// ─── Init / Lint ───

fn cmd_init(name: &str) -> Result<()> {
    let dir = Path::new(name);
    if dir.exists() {
        bail!("directory '{}' already exists", name);
    }
    agents::scaffold::init_agent(name, dir)?;
    println!("created {}/", name);
    println!("  Agentfile.toml");
    println!("  persona.md");
    println!("  skills/hello.sh");
    println!("  seed/");
    println!();
    println!("next: edit Agentfile.toml, then `aide.sh build {}/`", name);
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

    // Scan for credential leaks before building
    let leaks = scan_for_leaks(&dir)?;
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

async fn cmd_push(dir: &Path) -> Result<()> {
    let dir = std::fs::canonicalize(dir)
        .with_context(|| format!("directory not found: {}", dir.display()))?;

    let spec = AgentfileSpec::load(&dir)?;

    let auth_path = aide_home().join("auth.json");
    if !auth_path.exists() {
        bail!("not authenticated. Run `aide.sh login` first.");
    }
    let auth_content = std::fs::read_to_string(&auth_path)?;
    let auth: serde_json::Value = serde_json::from_str(&auth_content)
        .context("failed to parse ~/.aide/auth.json")?;
    let token = auth.get("token").and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("no token in ~/.aide/auth.json"))?;
    let username = auth.get("username").and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("no username in ~/.aide/auth.json"))?;

    let archive_name = spec.archive_name();
    let archive_path = aide_home().join("builds").join(&archive_name);
    if !archive_path.exists() {
        cmd_build(&dir)?;
    }

    let archive_bytes = std::fs::read(&archive_path)
        .with_context(|| format!("failed to read {}", archive_path.display()))?;

    println!("pushing {}/{}:{}", username, spec.agent.name, spec.agent.version);

    let url = format!("https://hub.aide.sh/v1/{}/{}", username, spec.agent.name);

    let part = reqwest::multipart::Part::bytes(archive_bytes)
        .file_name(archive_name.clone())
        .mime_str("application/gzip")?;
    let metadata = serde_json::json!({
        "name": spec.agent.name,
        "version": spec.agent.version,
        "description": spec.agent.description,
        "author": spec.agent.author,
        "skills": spec.skills.keys().collect::<Vec<_>>(),
    });
    let metadata_part = reqwest::multipart::Part::text(metadata.to_string())
        .mime_str("application/json")?;
    let form = reqwest::multipart::Form::new()
        .part("archive", part)
        .part("metadata", metadata_part);

    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", token))
        .multipart(form)
        .send()
        .await;

    match resp {
        Ok(r) if r.status().is_success() => {
            println!("{}/{}:{}", username, spec.agent.name, spec.agent.version);
        }
        Ok(r) => {
            let status = r.status();
            let body = r.text().await.unwrap_or_default();
            bail!("push failed ({}): {}", status, body);
        }
        Err(e) => {
            println!("push request failed: {}", e);
            println!("(registry at {} not yet available)", url);
        }
    }

    Ok(())
}

async fn cmd_pull(agent_ref: &str, version: &str) -> Result<()> {
    use flate2::read::GzDecoder;

    let parts: Vec<&str> = agent_ref.splitn(2, '/').collect();
    if parts.len() != 2 {
        bail!("invalid image '{}' — expected <user>/<type>", agent_ref);
    }
    let (user, agent_type) = (parts[0], parts[1]);

    let url = format!("https://hub.aide.sh/v1/{}/{}/{}", user, agent_type, version);

    println!("pulling {}:{}...", agent_ref, version);

    let client = reqwest::Client::new();
    let resp = client.get(&url).send().await;

    let archive_bytes = match resp {
        Ok(r) if r.status().is_success() => r.bytes().await?.to_vec(),
        Ok(r) => {
            let status = r.status();
            let body = r.text().await.unwrap_or_default();
            bail!("pull failed ({}): {}", status, body);
        }
        Err(e) => {
            bail!(
                "failed to reach registry: {}\n(place tarball in ~/.aide/types/{}/{}/ for local testing)",
                e, user, agent_type
            );
        }
    };

    let types_dir = aide_home().join("types").join(user).join(agent_type);
    if types_dir.exists() {
        std::fs::remove_dir_all(&types_dir)?;
    }
    std::fs::create_dir_all(&types_dir)?;

    let decoder = GzDecoder::new(&archive_bytes[..]);
    let mut archive = tar::Archive::new(decoder);
    archive.unpack(&types_dir)?;

    if let Ok(spec) = AgentfileSpec::load(&types_dir) {
        println!("{}:{}", agent_ref, spec.agent.version);
    } else {
        println!("{}", agent_ref);
    }

    Ok(())
}

async fn cmd_login() -> Result<()> {
    let client_id = "PLACEHOLDER_CLIENT_ID";

    println!("aide.sh login — authenticating via GitHub");
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
                "expired_token" => bail!("device code expired — run `aide.sh login` again"),
                "access_denied" => bail!("authorization denied"),
                _ => bail!("OAuth error: {}", error),
            }
        }

        if let Some(access_token) = token_resp.get("access_token").and_then(|v| v.as_str()) {
            let user_resp = client
                .get("https://api.github.com/user")
                .header("Authorization", format!("Bearer {}", access_token))
                .header("User-Agent", "aide-sh")
                .send().await?.json::<serde_json::Value>().await?;

            let username = user_resp["login"].as_str().unwrap_or("unknown").to_string();

            let auth = serde_json::json!({
                "token": access_token,
                "username": username,
                "provider": "github",
            });

            let auth_path = aide_home().join("auth.json");
            std::fs::create_dir_all(aide_home())?;
            std::fs::write(&auth_path, serde_json::to_string_pretty(&auth)?)?;

            println!("Login Succeeded ({})", username);
            return Ok(());
        }
    }
}

async fn cmd_search(query: &str) -> Result<()> {
    let url = format!("https://hub.aide.sh/v1/search?q={}", urlencoded(query));

    let client = reqwest::Client::new();
    let resp = client.get(&url).send().await;

    match resp {
        Ok(r) if r.status().is_success() => {
            let results: serde_json::Value = r.json().await?;
            if let Some(agents) = results.get("agents").and_then(|v| v.as_array()) {
                if agents.is_empty() {
                    println!("no results for '{}'", query);
                    return Ok(());
                }

                println!(
                    "{:<24} {:<10} {:<12} {}",
                    "NAME", "VERSION", "AUTHOR", "DESCRIPTION"
                );
                println!("{}", "─".repeat(72));

                for agent in agents {
                    println!(
                        "{:<24} {:<10} {:<12} {}",
                        agent["name"].as_str().unwrap_or("?"),
                        agent["version"].as_str().unwrap_or("?"),
                        agent["author"].as_str().unwrap_or("?"),
                        agent["description"].as_str().unwrap_or(""),
                    );
                }
            }
        }
        Ok(r) => {
            let status = r.status();
            let body = r.text().await.unwrap_or_default();
            bail!("search failed ({}): {}", status, body);
        }
        Err(e) => {
            bail!("failed to reach registry: {}", e);
        }
    }

    Ok(())
}

fn urlencoded(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                String::from(b as char)
            }
            b' ' => "+".to_string(),
            _ => format!("%{:02X}", b),
        })
        .collect()
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
