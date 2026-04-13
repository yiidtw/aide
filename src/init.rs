use anyhow::{Context, Result};
use dialoguer::{Confirm, Input, MultiSelect, theme::ColorfulTheme};
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct InitArgs {
    pub name: Option<String>,
    pub scan_dir: Option<String>,
    pub members: Option<String>,
    pub vault: Option<String>,
    pub skill_dir: Option<String>,
}

struct InitConfig {
    team_name: String,
    hq_dir: PathBuf,
    members: Vec<ProjectInfo>,
    vault_path: Option<PathBuf>,
    skill_dir: Option<PathBuf>,
    gh_user: Option<String>,
}

#[derive(Clone)]
struct ProjectInfo {
    name: String,
    path: PathBuf,
    has_aidefile: bool,
}

impl std::fmt::Display for ProjectInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let status = if self.has_aidefile { "✓" } else { "○" };
        write!(
            f,
            "{} {:<24} {}",
            status,
            self.name,
            self.path.display()
        )
    }
}

pub fn run(args: InitArgs) -> Result<()> {
    let theme = ColorfulTheme::default();
    let interactive = args.name.is_none() || args.members.is_none();

    println!();
    println!("\x1b[1maide — your commander of agents\x1b[0m");
    println!("─────────────────────────────────");
    println!();

    // ── Step 1: Prerequisites ──
    check_prerequisites()?;

    // ── Step 2: GitHub auth ──
    let gh_user = step_github_auth(if interactive { Some(&theme) } else { None })?;

    // ── Step 3: Scan for projects ──
    let scan_dir = match &args.scan_dir {
        Some(d) => PathBuf::from(shellexpand::tilde(d).to_string()),
        None if interactive => prompt_scan_dir(&theme)?,
        None => {
            // Non-interactive without scan_dir: use ~/projects
            let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
            home.join("projects")
        }
    };
    let projects = scan_projects(&scan_dir)?;

    if projects.is_empty() && args.members.is_none() {
        println!("  No projects found in {}", scan_dir.display());
        println!("  Create a project first, then run `aide init` again.");
        return Ok(());
    }

    // ── Step 4: Select members ──
    let selected = match &args.members {
        Some(m) => parse_members_flag(m, &projects)?,
        None => prompt_select_members(&theme, &projects)?,
    };

    if selected.is_empty() {
        println!("  No agents selected. Nothing to do.");
        return Ok(());
    }

    // ── Step 5: Team name ──
    let team_name = match args.name {
        Some(n) => n,
        None => prompt_team_name(&theme)?,
    };

    // ── Step 6: Vault ──
    let vault_path = match &args.vault {
        Some(v) => {
            let p = PathBuf::from(shellexpand::tilde(v).to_string());
            if p.exists() { Some(p) } else { None }
        }
        None => detect_vault(),
    };

    // ── Step 7: Skill directory ──
    let skill_dir = match &args.skill_dir {
        Some(s) => {
            let p = PathBuf::from(shellexpand::tilde(s).to_string());
            if p.exists() { Some(p) } else { None }
        }
        None => detect_skill_dir(),
    };

    // ── Create HQ ──
    let config = InitConfig {
        team_name,
        hq_dir: std::env::current_dir()?,
        members: selected,
        vault_path,
        skill_dir,
        gh_user,
    };

    create_hq(&config)?;

    Ok(())
}

// ── Prerequisites ──

fn check_prerequisites() -> Result<()> {
    print!("Checking prerequisites...");

    // Check gh CLI
    let gh = Command::new("gh").arg("--version").output();
    match gh {
        Ok(o) if o.status.success() => {
            println!(" \x1b[32m✓\x1b[0m gh CLI found");
        }
        _ => {
            println!(" \x1b[31m✗\x1b[0m gh CLI not found");
            println!();
            println!("  aide requires the GitHub CLI (gh).");
            println!("  Install: https://cli.github.com/");
            println!();
            println!("  macOS:   brew install gh");
            println!("  Linux:   https://github.com/cli/cli/blob/trunk/docs/install_linux.md");
            anyhow::bail!("gh CLI is required");
        }
    }

    // Check claude CLI
    let claude = Command::new("claude").arg("--version").output();
    match claude {
        Ok(o) if o.status.success() => {
            println!("                         \x1b[32m✓\x1b[0m claude CLI found");
        }
        _ => {
            println!("                         \x1b[33m!\x1b[0m claude CLI not found (optional for now)");
        }
    }

    Ok(())
}

// ── GitHub Auth ──

fn step_github_auth(theme: Option<&ColorfulTheme>) -> Result<Option<String>> {
    // Check if already logged in
    let status = Command::new("gh")
        .args(["auth", "status"])
        .output()
        .context("failed to run gh auth status")?;

    if status.status.success() {
        let out = String::from_utf8_lossy(&status.stdout);
        let err = String::from_utf8_lossy(&status.stderr);
        let combined = format!("{}{}", out, err);

        // Extract username
        let user = extract_gh_user(&combined);

        if let Some(ref u) = user {
            println!("\x1b[32m✓\x1b[0m GitHub: logged in as {}", u);
            return Ok(user);
        }
    }

    // Not logged in
    let Some(theme) = theme else {
        // Non-interactive: just warn and continue
        println!("  \x1b[33m!\x1b[0m Not logged in to GitHub. Run `gh auth login` first.");
        return Ok(None);
    };

    let login = Confirm::with_theme(theme)
        .with_prompt("Log in to GitHub? (required for dispatch)")
        .default(true)
        .interact()?;

    if !login {
        println!("  \x1b[33m!\x1b[0m Skipping GitHub auth. dispatch/wait/cancel won't work.");
        return Ok(None);
    }

    let result = Command::new("gh")
        .args(["auth", "login", "--web", "--scopes", "repo,read:org"])
        .status()
        .context("failed to run gh auth login")?;

    if !result.success() {
        println!("  \x1b[31m✗\x1b[0m GitHub auth failed");
        return Ok(None);
    }

    // Re-check to get username
    let status = Command::new("gh")
        .args(["auth", "status"])
        .output()?;
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&status.stdout),
        String::from_utf8_lossy(&status.stderr)
    );
    let user = extract_gh_user(&combined);

    if let Some(ref u) = user {
        println!("\x1b[32m✓\x1b[0m Logged in as {}", u);
    }

    Ok(user)
}

fn extract_gh_user(output: &str) -> Option<String> {
    output.lines().find_map(|l| {
        if l.contains("Logged in to") {
            l.split("as ")
                .nth(1)
                .map(|s| s.split_whitespace().next().unwrap_or("").to_string())
        } else {
            None
        }
    })
}

// ── Scan Projects ──

fn prompt_scan_dir(theme: &ColorfulTheme) -> Result<PathBuf> {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));

    // Try common project directories
    let candidates = ["projects", "src", "code", "dev", "repos", "claude_projects"];
    let default = candidates
        .iter()
        .map(|c| home.join(c))
        .find(|p| p.exists())
        .unwrap_or_else(|| home.join("projects"));

    let input: String = Input::with_theme(theme)
        .with_prompt("Where to scan for projects?")
        .default(default.display().to_string())
        .interact_text()?;

    let path = PathBuf::from(shellexpand::tilde(&input).to_string());
    if !path.exists() {
        anyhow::bail!("Directory does not exist: {}", path.display());
    }

    Ok(path)
}

fn scan_projects(dir: &Path) -> Result<Vec<ProjectInfo>> {
    println!("  Scanning {}...", dir.display());
    let mut projects = Vec::new();

    let entries = std::fs::read_dir(dir)
        .with_context(|| format!("cannot read {}", dir.display()))?;

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();

        // Skip hidden dirs and common non-project dirs
        if name.starts_with('.') || name == "node_modules" || name == "target" {
            continue;
        }

        let has_aidefile = path.join("Aidefile").exists();

        // Only include dirs that look like projects (have .git, Aidefile, or src/)
        let is_project = has_aidefile
            || path.join(".git").exists()
            || path.join("src").exists()
            || path.join("Cargo.toml").exists()
            || path.join("package.json").exists()
            || path.join("pyproject.toml").exists()
            || path.join("go.mod").exists();

        if is_project {
            projects.push(ProjectInfo {
                name,
                path,
                has_aidefile,
            });
        }
    }

    // Sort: Aidefile first, then alphabetical
    projects.sort_by(|a, b| {
        b.has_aidefile
            .cmp(&a.has_aidefile)
            .then(a.name.cmp(&b.name))
    });

    let with_aide = projects.iter().filter(|p| p.has_aidefile).count();
    let without = projects.len() - with_aide;
    println!(
        "  Found {} projects ({} with Aidefile, {} without)",
        projects.len(),
        with_aide,
        without
    );

    Ok(projects)
}

// ── Select Members ──

fn prompt_select_members(
    theme: &ColorfulTheme,
    projects: &[ProjectInfo],
) -> Result<Vec<ProjectInfo>> {
    if projects.is_empty() {
        return Ok(vec![]);
    }

    let labels: Vec<String> = projects.iter().map(|p| p.to_string()).collect();

    // Pre-select projects that already have Aidefile
    let defaults: Vec<bool> = projects.iter().map(|p| p.has_aidefile).collect();

    let selections = MultiSelect::with_theme(theme)
        .with_prompt("Select agents to register (space to toggle)")
        .items(&labels)
        .defaults(&defaults)
        .interact()?;

    let selected: Vec<ProjectInfo> = selections
        .into_iter()
        .map(|i| projects[i].clone())
        .collect();

    Ok(selected)
}

fn parse_members_flag(members: &str, projects: &[ProjectInfo]) -> Result<Vec<ProjectInfo>> {
    let paths: Vec<&str> = members.split(',').collect();
    let mut selected = Vec::new();

    for p in paths {
        let path = PathBuf::from(shellexpand::tilde(p.trim()).to_string());
        if let Some(proj) = projects.iter().find(|proj| proj.path == path) {
            selected.push(proj.clone());
        } else {
            // Not in scan results — create ProjectInfo from path
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("agent")
                .to_string();
            selected.push(ProjectInfo {
                name,
                path,
                has_aidefile: false,
            });
        }
    }

    Ok(selected)
}

// ── Team Name ──

fn prompt_team_name(theme: &ColorfulTheme) -> Result<String> {
    let name: String = Input::with_theme(theme)
        .with_prompt("Team name")
        .interact_text()?;

    Ok(name)
}

// ── Detection ──

fn detect_vault() -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    let candidates = [
        home.join(".aide").join("vault.toml"),
        home.join(".aide").join("vault"),
    ];
    candidates.into_iter().find(|p| p.exists())
}

fn detect_skill_dir() -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    let candidates = [
        home.join("aide-skill"),
        home.join("claude_projects").join("aide-skill"),
        home.join("projects").join("aide-skill"),
    ];
    candidates.into_iter().find(|p| p.exists())
}

// ── Create HQ ──

fn create_hq(config: &InitConfig) -> Result<()> {
    let hq_name = format!("{}-hq", config.team_name);
    let hq_dir = config.hq_dir.join(&hq_name);

    println!();
    println!("Creating {}/", hq_name);

    // Create directory structure
    std::fs::create_dir_all(hq_dir.join(".claude").join("agents"))?;
    std::fs::create_dir_all(hq_dir.join("memory").join("_shared"))?;

    // Create per-agent memory dirs
    for member in &config.members {
        std::fs::create_dir_all(hq_dir.join("memory").join(&member.name))?;
    }

    // Git init
    let git_dir = hq_dir.join(".git");
    if !git_dir.exists() {
        Command::new("git")
            .args(["init"])
            .current_dir(&hq_dir)
            .output()?;
        println!("  \x1b[32m✓\x1b[0m git init");
    }

    // CLAUDE.md
    let member_list: String = config
        .members
        .iter()
        .map(|m| format!("- `{}` — {}", m.name, m.path.display()))
        .collect::<Vec<_>>()
        .join("\n");

    let claude_md = format!(
        r#"# {} Team HQ

You are the coordinator for the {} team.
Do NOT do the work yourself — dispatch to member agents via `aide dispatch`.

## Dispatch workflow
1. `aide dispatch <agent> "<task>"` — dispatch work to a member
2. `aide wait <issue-ref>` — wait for result
3. `aide events` — check orchestration timeline
4. `aide cancel <issue-ref>` — cancel if stuck

## Member agents
{}

Run `aide list` to see all registered agents.
"#,
        config.team_name, config.team_name, member_list
    );
    std::fs::write(hq_dir.join("CLAUDE.md"), claude_md)?;
    println!("  \x1b[32m✓\x1b[0m CLAUDE.md");

    // .gitignore
    std::fs::write(
        hq_dir.join(".gitignore"),
        ".aide/\n.claude/\n!.claude/agents/\n!.claude/settings.json\n",
    )?;
    println!("  \x1b[32m✓\x1b[0m .gitignore");

    // Create Aidefiles for projects that don't have one
    for member in &config.members {
        if !member.has_aidefile {
            let aidefile_content = format!(
                r#"[persona]
name = "{}"

[budget]
tokens = "200k"
max_retries = 3

[trigger]
on = "manual"
"#,
                member.name
            );
            let aidefile_path = member.path.join("Aidefile");
            if !aidefile_path.exists() && member.path.exists() {
                std::fs::write(&aidefile_path, aidefile_content)?;
                println!(
                    "  \x1b[32m✓\x1b[0m Created Aidefile in {}",
                    member.path.display()
                );
            }
        }
    }

    // Generate .claude/agents/ wrappers
    for member in &config.members {
        let wrapper = format!(
            r#"---
name: {}
description: "Dispatch {} work via aide. Runs in isolated token budget."
tools:
  - Bash
---

You are a dispatch wrapper for the `{}` aide agent.

## Rules
- You ONLY run `aide dispatch` and `aide wait` commands
- Do NOT attempt to do the work yourself

## Workflow
1. Run: `aide dispatch {} "{{task}}"`
2. Capture the issue reference from output
3. Run: `aide wait {{issue_ref}}`
4. Return the summary to the user
"#,
            member.name, member.name, member.name, member.name
        );
        let wrapper_path = hq_dir.join(".claude").join("agents").join(format!("{}.md", member.name));
        std::fs::write(&wrapper_path, wrapper)?;
    }
    println!(
        "  \x1b[32m✓\x1b[0m .claude/agents/ ({} wrappers)",
        config.members.len()
    );

    // Register agents in aide registry
    for member in &config.members {
        if member.path.exists() {
            let _ = Command::new("aide")
                .args(["register", &member.path.display().to_string(), "--name", &member.name])
                .output();
        }
    }
    println!(
        "  \x1b[32m✓\x1b[0m Registered {} agents",
        config.members.len()
    );

    // Memory SSOT
    println!("  \x1b[32m✓\x1b[0m memory/ (team SSOT)");

    // Vault info
    if let Some(ref v) = config.vault_path {
        println!("  \x1b[32m✓\x1b[0m Vault: {}", v.display());
    }

    // Skill dir info
    if let Some(ref s) = config.skill_dir {
        println!("  \x1b[32m✓\x1b[0m Skills: {}", s.display());
    }

    // Done
    println!();
    println!("\x1b[32m\x1b[1m✓ {} created\x1b[0m", hq_name);
    println!();
    println!("  cd {} && claude", hq_name);
    println!();

    Ok(())
}
