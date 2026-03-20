// Hub configuration and operations
// hub = git repo containing occupation/ snapshots (git-native registry)

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

// ─── Hub config (persisted in ~/.aide/hubs.toml) ───

#[derive(Serialize, Deserialize, Clone)]
pub struct HubsFile {
    pub hub: Vec<HubConfig>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct HubConfig {
    pub name: String,
    pub repo: String, // e.g. "yiidtw/aide-hub"
    #[serde(default)]
    pub default: bool,
}

// ─── Index (lives at index.json in hub repo root) ───

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct HubIndex {
    pub agents: Vec<AgentEntry>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct AgentEntry {
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    #[serde(default)]
    pub skills: Vec<String>,
    pub updated_at: String,
}

// ─── Paths ───

fn aide_home() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".aide")
}

fn hubs_path() -> PathBuf {
    aide_home().join("hubs.toml")
}

// ─── Load / Save hubs ───

pub fn load_hubs() -> Vec<HubConfig> {
    let path = hubs_path();
    if !path.exists() {
        // Return default hub
        return vec![HubConfig {
            name: "aide.sh".to_string(),
            repo: "yiidtw/aide-hub".to_string(),
            default: true,
        }];
    }
    match std::fs::read_to_string(&path) {
        Ok(content) => match toml::from_str::<HubsFile>(&content) {
            Ok(f) => f.hub,
            Err(_) => vec![],
        },
        Err(_) => vec![],
    }
}

pub fn save_hubs(hubs: &[HubConfig]) -> Result<()> {
    let file = HubsFile {
        hub: hubs.to_vec(),
    };
    let content = toml::to_string_pretty(&file)?;
    std::fs::create_dir_all(aide_home())?;
    std::fs::write(hubs_path(), content)?;
    Ok(())
}

// ─── Add / Remove hub ───

pub fn add_hub(repo: &str) -> Result<()> {
    let mut hubs = load_hubs();

    // Derive name from repo: "acme-corp/aide-hub" → "acme-corp"
    let name = repo
        .split('/')
        .next()
        .unwrap_or(repo)
        .to_string();

    if hubs.iter().any(|h| h.repo == repo) {
        bail!("hub '{}' already configured", repo);
    }

    let is_first = hubs.is_empty();
    hubs.push(HubConfig {
        name,
        repo: repo.to_string(),
        default: is_first,
    });

    save_hubs(&hubs)?;
    Ok(())
}

pub fn remove_hub(name: &str) -> Result<bool> {
    let mut hubs = load_hubs();
    let before = hubs.len();
    hubs.retain(|h| h.name != name);
    if hubs.len() == before {
        return Ok(false);
    }
    // If we removed the default, make the first one default
    if !hubs.is_empty() && !hubs.iter().any(|h| h.default) {
        hubs[0].default = true;
    }
    save_hubs(&hubs)?;
    Ok(true)
}

// ─── Git helper: run git command ───

fn git_cmd(args: &[&str], cwd: &Path) -> Result<String> {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .with_context(|| format!("failed to run git {:?}", args))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git {:?} failed: {}", args, stderr);
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn gh_token() -> Option<String> {
    // Try auth.json first
    let auth_path = aide_home().join("auth.json");
    if let Ok(content) = std::fs::read_to_string(&auth_path) {
        if let Ok(auth) = serde_json::from_str::<serde_json::Value>(&content) {
            if let Some(token) = auth.get("token").and_then(|v| v.as_str()) {
                return Some(token.to_string());
            }
        }
    }
    // Fall back to GH_TOKEN / GITHUB_TOKEN env
    std::env::var("GH_TOKEN")
        .or_else(|_| std::env::var("GITHUB_TOKEN"))
        .ok()
}

fn clone_url(repo: &str, token: Option<&str>) -> String {
    match token {
        Some(t) => format!("https://x-access-token:{}@github.com/{}.git", t, repo),
        None => format!("https://github.com/{}.git", repo),
    }
}

// ─── Push occupation/ to hub ───

pub fn push_to_hub(agent_name: &str, occupation_dir: &Path, hub_repo: &str) -> Result<()> {
    let token = gh_token()
        .ok_or_else(|| anyhow::anyhow!("not authenticated. Run `aide login` first."))?;

    let tmp_dir = std::env::temp_dir().join(format!("aide-hub-push-{}", agent_name));
    if tmp_dir.exists() {
        std::fs::remove_dir_all(&tmp_dir)?;
    }

    // 1. Clone hub repo
    let url = clone_url(hub_repo, Some(&token));
    let output = std::process::Command::new("git")
        .args(["clone", "--depth", "1", &url, &tmp_dir.to_string_lossy()])
        .output()
        .context("failed to clone hub repo")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("failed to clone hub repo '{}': {}", hub_repo, stderr);
    }

    // 2. Copy occupation/ files to agents/{agent_name}/
    let agent_dir = tmp_dir.join("agents").join(agent_name);
    if agent_dir.exists() {
        std::fs::remove_dir_all(&agent_dir)?;
    }
    copy_dir_recursive(occupation_dir, &agent_dir)?;

    // 3. Read auth for author name
    let author = read_username().unwrap_or_else(|| "unknown".to_string());

    // 4. Update index.json
    let index_path = tmp_dir.join("index.json");
    let mut index = if index_path.exists() {
        let content = std::fs::read_to_string(&index_path)?;
        serde_json::from_str::<HubIndex>(&content).unwrap_or_default()
    } else {
        HubIndex::default()
    };

    // Read Agentfile.toml for metadata
    let (version, description, skills) = read_agent_metadata(occupation_dir);

    // Upsert entry
    index.agents.retain(|a| a.name != agent_name);
    index.agents.push(AgentEntry {
        name: agent_name.to_string(),
        version,
        description,
        author: author.clone(),
        skills,
        updated_at: chrono::Utc::now().to_rfc3339(),
    });

    std::fs::write(&index_path, serde_json::to_string_pretty(&index)?)?;

    // 5. Git commit + push
    git_cmd(&["add", "-A"], &tmp_dir)?;
    let msg = format!("push {}", agent_name);
    // Commit (ignore error if nothing changed)
    let _ = std::process::Command::new("git")
        .args(["commit", "-m", &msg])
        .current_dir(&tmp_dir)
        .output();
    git_cmd(&["push", "origin", "HEAD"], &tmp_dir)?;

    // 6. Cleanup
    std::fs::remove_dir_all(&tmp_dir)?;

    Ok(())
}

// ─── Pull occupation/ from hub ───

pub fn pull_from_hub(agent_name: &str, hub_repo: &str) -> Result<PathBuf> {
    let token = gh_token();

    let tmp_dir = std::env::temp_dir().join(format!("aide-hub-pull-{}", agent_name));
    if tmp_dir.exists() {
        std::fs::remove_dir_all(&tmp_dir)?;
    }

    let url = clone_url(hub_repo, token.as_deref());

    // Sparse checkout: only agents/{agent_name}/
    std::fs::create_dir_all(&tmp_dir)?;
    git_cmd(&["init"], &tmp_dir)?;
    git_cmd(&["remote", "add", "origin", &url], &tmp_dir)?;
    git_cmd(
        &["sparse-checkout", "set", &format!("agents/{}", agent_name)],
        &tmp_dir,
    )?;
    git_cmd(&["pull", "origin", "main", "--depth", "1"], &tmp_dir)
        .or_else(|_| git_cmd(&["pull", "origin", "master", "--depth", "1"], &tmp_dir))?;

    let agent_dir = tmp_dir.join("agents").join(agent_name);
    if !agent_dir.exists() {
        std::fs::remove_dir_all(&tmp_dir)?;
        bail!(
            "agent '{}' not found in hub '{}'",
            agent_name, hub_repo
        );
    }

    // Copy to ~/.aide/types/<hub-owner>/<agent_name>/
    let hub_owner = hub_repo.split('/').next().unwrap_or("hub");
    let types_dir = aide_home().join("types").join(hub_owner).join(agent_name);
    if types_dir.exists() {
        std::fs::remove_dir_all(&types_dir)?;
    }
    std::fs::create_dir_all(&types_dir)?;
    copy_dir_recursive(&agent_dir, &types_dir)?;

    // Cleanup
    std::fs::remove_dir_all(&tmp_dir)?;

    Ok(types_dir)
}

// ─── Search agents in hub ───

pub fn search_hub(query: &str, hub_repo: &str) -> Result<Vec<AgentEntry>> {
    let token = gh_token();

    // Fetch index.json via GitHub raw content API
    let index = fetch_index(hub_repo, token.as_deref())?;

    let query_lower = query.to_lowercase();
    let results: Vec<AgentEntry> = index
        .agents
        .into_iter()
        .filter(|a| {
            a.name.to_lowercase().contains(&query_lower)
                || a.description.to_lowercase().contains(&query_lower)
                || a.skills
                    .iter()
                    .any(|s| s.to_lowercase().contains(&query_lower))
        })
        .collect();

    Ok(results)
}

fn fetch_index(hub_repo: &str, token: Option<&str>) -> Result<HubIndex> {
    // Use git archive or raw URL to fetch index.json without cloning
    let raw_url = format!(
        "https://raw.githubusercontent.com/{}/main/index.json",
        hub_repo
    );

    let mut cmd = std::process::Command::new("curl");
    cmd.args(["-sfL", &raw_url]);
    if let Some(t) = token {
        cmd.args(["-H", &format!("Authorization: token {}", t)]);
    }

    let output = cmd.output().context("failed to fetch index.json")?;

    if !output.status.success() {
        // Try master branch
        let raw_url_master = format!(
            "https://raw.githubusercontent.com/{}/master/index.json",
            hub_repo
        );
        let mut cmd2 = std::process::Command::new("curl");
        cmd2.args(["-sfL", &raw_url_master]);
        if let Some(t) = token {
            cmd2.args(["-H", &format!("Authorization: token {}", t)]);
        }
        let output2 = cmd2.output()?;
        if !output2.status.success() {
            bail!("failed to fetch index.json from hub '{}'", hub_repo);
        }
        let content = String::from_utf8_lossy(&output2.stdout);
        return Ok(serde_json::from_str(&content)?);
    }

    let content = String::from_utf8_lossy(&output.stdout);
    Ok(serde_json::from_str(&content)?)
}

// ─── Initialize a new hub repo ───

pub fn init_hub(repo_name: &str, visibility: &str) -> Result<()> {
    // 1. Create repo via gh CLI
    let vis_flag = format!("--{}", visibility);
    let output = std::process::Command::new("gh")
        .args(["repo", "create", repo_name, &vis_flag, "--confirm"])
        .output()
        .context("failed to run `gh repo create`. Is gh CLI installed?")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !stderr.contains("already exists") {
            bail!("failed to create repo: {}", stderr);
        }
        println!("  repo already exists, initializing...");
    }

    // 2. Clone and initialize with index.json + README
    let tmp_dir = std::env::temp_dir().join(format!("aide-hub-init-{}", repo_name.replace('/', "-")));
    if tmp_dir.exists() {
        std::fs::remove_dir_all(&tmp_dir)?;
    }

    let token = gh_token();
    let url = clone_url(repo_name, token.as_deref());

    // Try clone first (repo might have content already)
    let clone_output = std::process::Command::new("git")
        .args(["clone", &url, &tmp_dir.to_string_lossy()])
        .output()?;

    if !clone_output.status.success() {
        // Empty repo, init from scratch
        std::fs::create_dir_all(&tmp_dir)?;
        git_cmd(&["init"], &tmp_dir)?;
        git_cmd(&["remote", "add", "origin", &url], &tmp_dir)?;
    }

    // Create agents/ directory
    std::fs::create_dir_all(tmp_dir.join("agents"))?;
    std::fs::write(tmp_dir.join("agents/.gitkeep"), "")?;

    // Create index.json
    let index = HubIndex { agents: vec![] };
    std::fs::write(
        tmp_dir.join("index.json"),
        serde_json::to_string_pretty(&index)?,
    )?;

    // Create README
    let name = repo_name.split('/').last().unwrap_or(repo_name);
    let readme = format!(
        "# {}\n\nAn [aide.sh](https://aide.sh) agent hub.\n\n\
         ## Usage\n\n\
         ```bash\n\
         aide hub add {}\n\
         aide search <query>\n\
         aide pull <agent-name>\n\
         ```\n",
        name, repo_name
    );
    std::fs::write(tmp_dir.join("README.md"), readme)?;

    // Commit and push
    git_cmd(&["add", "-A"], &tmp_dir)?;
    let _ = std::process::Command::new("git")
        .args(["commit", "-m", "init aide hub"])
        .current_dir(&tmp_dir)
        .output();
    git_cmd(&["branch", "-M", "main"], &tmp_dir)?;
    git_cmd(&["push", "-u", "origin", "main"], &tmp_dir)
        .or_else(|_| git_cmd(&["push", "-u", "origin", "main", "--force"], &tmp_dir))?;

    // Cleanup
    std::fs::remove_dir_all(&tmp_dir)?;

    Ok(())
}

// ─── Helpers ───

fn read_username() -> Option<String> {
    let auth_path = aide_home().join("auth.json");
    let content = std::fs::read_to_string(&auth_path).ok()?;
    let auth: serde_json::Value = serde_json::from_str(&content).ok()?;
    auth.get("username")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

fn read_agent_metadata(dir: &Path) -> (String, String, Vec<String>) {
    let agentfile = dir.join("Agentfile.toml");
    if !agentfile.exists() {
        return ("0.1.0".to_string(), String::new(), vec![]);
    }
    match std::fs::read_to_string(&agentfile) {
        Ok(content) => {
            // Parse just enough to get metadata
            if let Ok(val) = content.parse::<toml::Value>() {
                let version = val
                    .get("agent")
                    .and_then(|a| a.get("version"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("0.1.0")
                    .to_string();
                let description = val
                    .get("agent")
                    .and_then(|a| a.get("description"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let skills: Vec<String> = val
                    .get("skills")
                    .and_then(|s| s.as_table())
                    .map(|t| t.keys().cloned().collect())
                    .unwrap_or_default();
                (version, description, skills)
            } else {
                ("0.1.0".to_string(), String::new(), vec![])
            }
        }
        Err(_) => ("0.1.0".to_string(), String::new(), vec![]),
    }
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
