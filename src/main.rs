mod aidefile;
mod api;
mod budget;
mod daemon;
mod dashboard;
mod db;
mod dispatch;
mod emit;
mod events;
mod init;
mod mcp;
mod registry;
mod runner;
mod vault;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(
    name = "aide",
    about = "One file to agentize your Claude project.",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Execute a task in an agent directory
    Run {
        /// Agent name or path to directory with Aidefile
        agent: String,
        /// Task description
        task: String,
    },

    /// Create a new agent in ~/.aide/
    Spawn {
        /// Agent name
        name: String,
        /// Persona name for Aidefile
        #[arg(long)]
        persona: Option<String>,
    },

    /// Register an existing project as an agent
    Register {
        /// Path to directory with Aidefile
        path: String,
        /// Agent name (defaults to directory name)
        #[arg(long)]
        name: Option<String>,
    },

    /// Unregister an agent (does not delete files)
    Unregister {
        /// Agent name
        name: String,
    },

    /// List all managed agents
    List,

    /// Start the daemon (trigger polling loop)
    Up,

    /// Stop the daemon
    Down,

    /// Import a team template from a git URL
    Import {
        /// Git URL to clone
        url: String,
    },

    /// Export agents as a shareable template
    Export {
        /// Output directory
        #[arg(long)]
        to: String,
        /// Only export agents matching this tag/name
        #[arg(long)]
        name: Option<String>,
    },

    /// Initialize a team HQ — interactive setup or CLI flags
    Init {
        /// Team name (skip interactive prompt)
        #[arg(long)]
        name: Option<String>,
        /// Directory to scan for projects (default: ~/projects)
        #[arg(long)]
        scan_dir: Option<String>,
        /// Comma-separated list of agent paths to register
        #[arg(long)]
        members: Option<String>,
        /// Path to vault file
        #[arg(long)]
        vault: Option<String>,
        /// Path to skill directory
        #[arg(long)]
        skill_dir: Option<String>,
    },

    /// Start MCP stdio server for LLM tool integration
    Mcp,

    /// Vault operations
    Vault {
        #[command(subcommand)]
        command: VaultCommands,
    },

    /// Dispatch a task to an agent by creating a GitHub issue (frontier-driven)
    Dispatch {
        /// Agent name (must be registered)
        agent: String,
        /// Task description (first line becomes issue title)
        task: String,
        /// Print what would be dispatched without creating the issue
        #[arg(long)]
        dry_run: bool,
    },

    /// Block until a dispatched issue is closed; print summary to stdout
    Wait {
        /// Issue reference: `owner/repo#N` or full GitHub URL
        issue: String,
        /// Max wait duration (e.g. "10m", "1h")
        #[arg(long, default_value = "30m")]
        timeout: String,
        /// Poll interval (e.g. "5s", "30s")
        #[arg(long, default_value = "5s")]
        poll_interval: String,
    },

    /// Run one dispatched issue synchronously (internal, used by dispatch)
    #[command(hide = true)]
    RunIssue {
        /// Issue reference: `owner/repo#N` or full GitHub URL
        issue: String,
    },

    /// Cancel a running dispatched issue (kill worker, close issue)
    Cancel {
        /// Issue reference: `owner/repo#N` or full GitHub URL
        issue: String,
    },

    /// Show recent orchestration events (dispatch timeline)
    Events {
        /// Max number of events to show
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },

    /// Start the local HTTP API server
    Api {
        /// Port to listen on
        #[arg(long, default_value_t = 7979)]
        port: u16,
    },

    /// Show today's stats (runs, tokens, agents)
    Stats,

    /// Generate .claude/agents/*.md wrappers for all registered agents
    EmitClaudeAgents {
        /// Output directory (default: .claude/agents)
        #[arg(short, long, default_value = ".claude/agents")]
        output: String,
    },

    /// Show daemon health and last heartbeat
    Status,

    /// Install aide as a system service (macOS launchd / Linux systemd)
    InstallService,

    /// Uninstall aide system service
    UninstallService,

    /// Live TUI dashboard
    Dashboard,
}

#[derive(Subcommand)]
enum VaultCommands {
    /// Get a secret value by key
    Get {
        /// Key name
        key: String,
    },
    /// List all keys in the vault (names only)
    List,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Run { agent, task } => cmd_run(&agent, &task)?,
        Commands::Spawn { name, persona } => cmd_spawn(&name, persona.as_deref())?,
        Commands::Register { path, name } => cmd_register(&path, name.as_deref())?,
        Commands::Unregister { name } => cmd_unregister(&name)?,
        Commands::List => cmd_list()?,
        Commands::Up => daemon::start().await?,
        Commands::Down => daemon::stop()?,
        Commands::Import { url } => cmd_import(&url)?,
        Commands::Export { to, name } => cmd_export(&to, name.as_deref())?,
        Commands::Init { name, scan_dir, members, vault, skill_dir } => {
            init::run(init::InitArgs {
                name,
                scan_dir,
                members,
                vault,
                skill_dir,
            })?;
        }
        Commands::Mcp => mcp::serve()?,
        Commands::Vault { command } => match command {
            VaultCommands::Get { key } => {
                let val = vault::get(&key)?;
                print!("{val}");
            }
            VaultCommands::List => {
                let keys = vault::list_keys()?;
                for k in keys {
                    println!("{k}");
                }
            }
        },
        Commands::Dispatch { agent, task, dry_run } => {
            dispatch::dispatch(&agent, &task, dry_run)?;
        }
        Commands::Wait {
            issue,
            timeout,
            poll_interval,
        } => {
            let timeout_dur = aidefile::parse_duration(&timeout);
            let poll_dur = aidefile::parse_duration(&poll_interval);
            let code = dispatch::wait(&issue, timeout_dur, poll_dur, None)?;
            std::process::exit(code);
        }
        Commands::RunIssue { issue } => {
            dispatch::run_issue(&issue)?;
        }
        Commands::Cancel { issue } => {
            dispatch::cancel(&issue)?;
        }
        Commands::Events { limit } => {
            let evs = events::recent(limit)?;
            events::print_timeline(&evs);
        }
        Commands::Api { port } => {
            api::serve(port).await?;
        }
        Commands::EmitClaudeAgents { output } => {
            emit::emit_claude_agents(&output)?;
        }
        Commands::Stats => cmd_stats()?,
        Commands::Status => cmd_status()?,
        Commands::InstallService => cmd_install_service()?,
        Commands::UninstallService => cmd_uninstall_service()?,
        Commands::Dashboard => dashboard::run_dashboard()?,
    }

    Ok(())
}

fn cmd_run(agent: &str, task: &str) -> Result<()> {
    let dir = registry::resolve(agent)?;
    println!("▸ Running task in {}", dir.display());

    let af = aidefile::load(&dir)?;
    println!("  agent: {}", af.persona.name);
    println!("  budget: {} tokens", af.budget.tokens_limit());

    let result = runner::run(&dir, task)?;

    if result.success {
        println!("✓ Task completed ({} tokens used)", result.tokens_used);
    } else {
        println!(
            "✗ Task incomplete ({} tokens used, budget exhausted)",
            result.tokens_used
        );
    }
    println!("\n── summary ──");
    println!("{}", result.summary);
    Ok(())
}

fn cmd_spawn(name: &str, persona: Option<&str>) -> Result<()> {
    let dir = registry::aide_dir().join(name);
    if dir.exists() {
        anyhow::bail!("Agent '{}' already exists at {}", name, dir.display());
    }

    std::fs::create_dir_all(&dir)?;

    // Write Aidefile
    let persona_name = persona.unwrap_or(name);
    let aidefile_content = format!(
        r#"[persona]
name = "{persona_name}"

[budget]
tokens = "200k"

[memory]
compact_after = "200k"

[trigger]
on = "manual"
"#
    );
    std::fs::write(dir.join("Aidefile"), aidefile_content)?;

    // Write minimal CLAUDE.md
    let claude_md = format!("# {persona_name}\n\nThis agent is managed by aide.\n");
    std::fs::write(dir.join("CLAUDE.md"), claude_md)?;

    // Create memory/ and skills/ dirs
    std::fs::create_dir_all(dir.join("memory"))?;
    std::fs::create_dir_all(dir.join("skills"))?;

    // Register
    registry::register(name, &dir)?;

    println!("✓ Spawned agent '{name}' at {}", dir.display());
    println!("  Edit {}/Aidefile to configure", dir.display());
    Ok(())
}

fn cmd_register(path: &str, name: Option<&str>) -> Result<()> {
    let path = PathBuf::from(shellexpand::tilde(path).as_ref());
    if !aidefile::exists(&path) {
        anyhow::bail!("No Aidefile found in {}", path.display());
    }

    let name = name
        .map(String::from)
        .or_else(|| {
            path.file_name()
                .map(|n| n.to_string_lossy().to_string())
        })
        .unwrap_or_else(|| "unnamed".into());

    registry::register(&name, &path)?;
    println!("✓ Registered '{}' → {}", name, path.display());
    Ok(())
}

fn cmd_unregister(name: &str) -> Result<()> {
    registry::unregister(name)?;
    println!("✓ Unregistered '{name}' (files not deleted)");
    Ok(())
}

fn cmd_list() -> Result<()> {
    let agents = registry::list()?;
    if agents.is_empty() {
        println!("No agents registered. Use `aide spawn` or `aide register`.");
        return Ok(());
    }

    println!("{:<20} {:<50} {}", "NAME", "PATH", "STATUS");
    println!("{}", "─".repeat(80));
    for agent in agents {
        let path = PathBuf::from(shellexpand::tilde(&agent.path).as_ref());
        let status = if aidefile::exists(&path) {
            let af = aidefile::load(&path).ok();
            af.map(|a| a.trigger.on.clone())
                .unwrap_or_else(|| "error".into())
        } else {
            "missing".into()
        };
        println!("{:<20} {:<50} {}", agent.name, agent.path, status);
    }
    Ok(())
}

fn cmd_import(url: &str) -> Result<()> {
    let tmp = tempfile::tempdir()?;
    println!("▸ Cloning {url}...");

    let status = std::process::Command::new("git")
        .args(["clone", "--depth=1", url])
        .arg(tmp.path())
        .status()
        .context("Failed to run git clone")?;

    if !status.success() {
        anyhow::bail!("git clone failed");
    }

    // Find all directories with Aidefiles and register them
    let mut count = 0;
    for entry in std::fs::read_dir(tmp.path())?.flatten() {
        if entry.file_type()?.is_dir() && aidefile::exists(&entry.path()) {
            let name = entry.file_name().to_string_lossy().to_string();
            let dest = registry::aide_dir().join(&name);

            // Copy to ~/.aide/
            copy_dir_recursive(&entry.path(), &dest)?;
            registry::register(&name, &dest)?;
            println!("  ✓ Imported '{name}'");
            count += 1;
        }
    }

    // Also check root
    if aidefile::exists(tmp.path()) {
        let name = url
            .rsplit('/')
            .next()
            .unwrap_or("imported")
            .trim_end_matches(".git");
        let dest = registry::aide_dir().join(name);
        copy_dir_recursive(tmp.path(), &dest)?;
        registry::register(name, &dest)?;
        println!("  ✓ Imported '{name}'");
        count += 1;
    }

    if count == 0 {
        println!("⚠ No Aidefiles found in repository");
    } else {
        println!("✓ Imported {count} agent(s)");
    }
    Ok(())
}

fn cmd_export(to: &str, name: Option<&str>) -> Result<()> {
    let dest = PathBuf::from(shellexpand::tilde(to).as_ref());
    std::fs::create_dir_all(&dest)?;

    let agents = registry::list()?;
    let agents: Vec<_> = if let Some(filter) = name {
        agents.into_iter().filter(|a| a.name == filter).collect()
    } else {
        agents
    };

    for agent in &agents {
        let src = PathBuf::from(shellexpand::tilde(&agent.path).as_ref());
        let agent_dest = dest.join(&agent.name);
        std::fs::create_dir_all(&agent_dest)?;

        // Copy only shareable files: Aidefile, CLAUDE.md, skills/
        for file in ["Aidefile", "CLAUDE.md"] {
            let f = src.join(file);
            if f.exists() {
                std::fs::copy(&f, agent_dest.join(file))?;
            }
        }
        let skills_src = src.join("skills");
        if skills_src.exists() {
            copy_dir_recursive(&skills_src, &agent_dest.join("skills"))?;
        }
        // Explicitly skip: memory/, vault.*, .git/
        println!("  ✓ Exported '{}'", agent.name);
    }

    println!("✓ Exported {} agent(s) to {}", agents.len(), dest.display());
    Ok(())
}

// cmd_init moved to init.rs

/// Recursively copy a directory.
fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) -> Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)?.flatten() {
        let dest_path = dst.join(entry.file_name());
        let ft = entry.file_type()?;
        if ft.is_dir() {
            let name = entry.file_name();
            // Skip .git, memory, vault files
            if name == ".git" || name == "memory" {
                continue;
            }
            copy_dir_recursive(&entry.path(), &dest_path)?;
        } else if ft.is_file() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.starts_with("vault") || name_str.ends_with(".age") {
                continue;
            }
            std::fs::copy(entry.path(), dest_path)?;
        }
    }
    Ok(())
}

fn cmd_stats() -> Result<()> {
    let stats = db::stats_today()?;
    println!("Date: {}", stats.date);
    println!("Runs: {} ({} success, {} failed)", stats.total_runs, stats.successful, stats.failed);
    println!("Tokens: {}k", stats.total_tokens / 1000);
    println!(
        "Agents: {}",
        if stats.agents_used.is_empty() {
            "(none)".into()
        } else {
            stats.agents_used.join(", ")
        }
    );

    // Dispatch telemetry summary
    if let Ok(t) = db::telemetry_summary() {
        if t.total_runs > 0 {
            println!();
            println!("── dispatch telemetry ──");
            println!("Measured runs: {}", t.total_runs);
            println!("Avg compression ratio: {:.4} (summary chars / sub-agent tokens)", t.avg_compression_ratio);
            println!("Sub-agent tokens: {}k", t.total_sub_agent_tokens / 1000);
            println!("Frontier wait tokens: {}k", t.total_frontier_wait_tokens / 1000);
            println!("Tokens saved: {}k", t.tokens_saved / 1000);
            println!("Savings multiplier: {:.1}x", t.savings_multiplier);
        }
    }

    Ok(())
}

fn cmd_status() -> Result<()> {
    match db::last_heartbeat()? {
        Some(hb) => {
            let age = if let Ok(ts) = chrono::DateTime::parse_from_rfc3339(&hb.ts) {
                let secs = (chrono::Utc::now() - ts.with_timezone(&chrono::Utc)).num_seconds();
                if secs < 60 {
                    format!("{secs}s ago")
                } else {
                    format!("{}m ago", secs / 60)
                }
            } else {
                "?".into()
            };
            let alive = if age.contains("ago") {
                let secs_val: i64 = age.split(|c: char| !c.is_ascii_digit()).next().unwrap_or("999").parse().unwrap_or(999);
                if age.contains('m') { secs_val < 3 } else { secs_val < 120 }
            } else {
                false
            };
            println!(
                "daemon: {} (PID {}, heartbeat {})",
                if alive { "running" } else { "stale" },
                hb.daemon_pid,
                age,
            );
            println!("agents: {}", hb.agents_count);
            println!("uptime: {}m", hb.uptime_secs / 60);
        }
        None => {
            println!("daemon: not running (no heartbeat found)");
        }
    }

    // Also show quick stats
    let stats = db::stats_today()?;
    if stats.total_runs > 0 {
        println!(
            "today: {} runs, {}k tokens",
            stats.total_runs,
            stats.total_tokens / 1000
        );
    }
    Ok(())
}

fn cmd_install_service() -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        let plist_dir = dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("No home dir"))?
            .join("Library/LaunchAgents");
        std::fs::create_dir_all(&plist_dir)?;

        let exe = std::env::current_exe()?;
        let plist_path = plist_dir.join("sh.aide.daemon.plist");

        let plist = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>sh.aide.daemon</string>
    <key>ProgramArguments</key>
    <array>
        <string>{exe}</string>
        <string>up</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardOutPath</key>
    <string>{home}/.aide/logs/daemon-stdout.log</string>
    <key>StandardErrorPath</key>
    <string>{home}/.aide/logs/daemon-stderr.log</string>
    <key>EnvironmentVariables</key>
    <dict>
        <key>PATH</key>
        <string>/usr/local/bin:/usr/bin:/bin:{home}/.local/bin</string>
    </dict>
</dict>
</plist>"#,
            exe = exe.display(),
            home = dirs::home_dir().unwrap().display(),
        );

        // Ensure logs dir exists
        let log_dir = registry::aide_dir().join("logs");
        std::fs::create_dir_all(&log_dir)?;

        std::fs::write(&plist_path, plist)?;

        // Load the service
        let status = std::process::Command::new("launchctl")
            .args(["load", "-w"])
            .arg(&plist_path)
            .status()?;

        if status.success() {
            println!("✓ Installed and loaded sh.aide.daemon");
            println!("  plist: {}", plist_path.display());
            println!("  The daemon will start on login and restart if it crashes.");
            println!("  Use `aide uninstall-service` to remove.");
        } else {
            println!("⚠ Plist written but launchctl load failed. Try manually:");
            println!("  launchctl load -w {}", plist_path.display());
        }
        return Ok(());
    }

    #[cfg(target_os = "linux")]
    {
        let systemd_dir = dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("No home dir"))?
            .join(".config/systemd/user");
        std::fs::create_dir_all(&systemd_dir)?;

        let exe = std::env::current_exe()?;
        let unit_path = systemd_dir.join("aide.service");

        let unit = format!(
            "[Unit]\nDescription=aide daemon\n\n[Service]\nExecStart={exe} up\nRestart=always\nRestartSec=5\n\n[Install]\nWantedBy=default.target\n",
            exe = exe.display(),
        );

        std::fs::write(&unit_path, unit)?;

        let _ = std::process::Command::new("systemctl")
            .args(["--user", "daemon-reload"])
            .status();
        let status = std::process::Command::new("systemctl")
            .args(["--user", "enable", "--now", "aide"])
            .status()?;

        if status.success() {
            println!("✓ Installed and started aide.service");
        } else {
            println!("⚠ Unit written but systemctl failed.");
        }
        return Ok(());
    }

    #[allow(unreachable_code)]
    {
        anyhow::bail!("install-service is only supported on macOS and Linux");
    }
}

fn cmd_uninstall_service() -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        let plist_path = dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("No home dir"))?
            .join("Library/LaunchAgents/sh.aide.daemon.plist");

        if plist_path.exists() {
            let _ = std::process::Command::new("launchctl")
                .args(["unload", "-w"])
                .arg(&plist_path)
                .status();
            std::fs::remove_file(&plist_path)?;
            println!("✓ Unloaded and removed sh.aide.daemon");
        } else {
            println!("No service installed (plist not found)");
        }
        return Ok(());
    }

    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new("systemctl")
            .args(["--user", "disable", "--now", "aide"])
            .status();
        let unit_path = dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("No home dir"))?
            .join(".config/systemd/user/aide.service");
        if unit_path.exists() {
            std::fs::remove_file(&unit_path)?;
        }
        let _ = std::process::Command::new("systemctl")
            .args(["--user", "daemon-reload"])
            .status();
        println!("✓ Removed aide.service");
        return Ok(());
    }

    #[allow(unreachable_code)]
    {
        anyhow::bail!("uninstall-service is only supported on macOS and Linux");
    }
}
