mod aidefile;
mod budget;
mod daemon;
mod dispatch;
mod events;
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

    /// Initialize an Aidefile in the current directory
    Init {
        /// Persona name
        #[arg(long)]
        persona: Option<String>,
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

    /// Show recent orchestration events (dispatch timeline)
    Events {
        /// Max number of events to show
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
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
        Commands::Init { persona } => cmd_init(persona.as_deref())?,
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
            let code = dispatch::wait(&issue, timeout_dur, poll_dur)?;
            std::process::exit(code);
        }
        Commands::RunIssue { issue } => {
            dispatch::run_issue(&issue)?;
        }
        Commands::Events { limit } => {
            let evs = events::recent(limit)?;
            events::print_timeline(&evs);
        }
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

fn cmd_init(persona: Option<&str>) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let aidefile_path = cwd.join("Aidefile");
    if aidefile_path.exists() {
        anyhow::bail!("Aidefile already exists in current directory");
    }

    let name = persona
        .or_else(|| cwd.file_name().map(|n| n.to_str().unwrap_or("agent")))
        .unwrap_or("agent");

    let content = format!(
        r#"[persona]
name = "{name}"
# style = "direct, concise"

[budget]
tokens = "200k"
max_retries = 3

[memory]
compact_after = "200k"

# [hooks]
# on_spawn = ["inject-vault"]
# on_complete = ["commit-memory"]

# [skills]
# include = ["code-review"]

[trigger]
on = "manual"

# [vault]
# keys = ["GITHUB_TOKEN"]
"#
    );

    std::fs::write(&aidefile_path, content)?;
    println!("✓ Created Aidefile in {}", cwd.display());
    println!("  Edit it, then run `aide register .` to activate");
    Ok(())
}

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
