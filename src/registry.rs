//! Agent registry — tracks which directories aide manages.
//!
//! Just a TOML config file at ~/.aide/config.toml.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const CONFIG_FILE: &str = "config.toml";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub daemon: DaemonConfig,
    #[serde(default)]
    pub agents: Vec<AgentEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonConfig {
    #[serde(default = "DaemonConfig::default_poll_interval")]
    pub poll_interval: String,
    /// Maximum number of concurrent agent tasks (default 8).
    #[serde(default = "DaemonConfig::default_max_concurrent")]
    pub max_concurrent: usize,
}

impl DaemonConfig {
    fn default_poll_interval() -> String {
        "60s".into()
    }
    fn default_max_concurrent() -> usize {
        8
    }
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            poll_interval: Self::default_poll_interval(),
            max_concurrent: Self::default_max_concurrent(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentEntry {
    pub name: String,
    pub path: String,
}

/// Path to config directory.
pub fn aide_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_default()
        .join(".aide")
}

/// Load config from ~/.aide/config.toml.
pub fn load() -> Result<Config> {
    let path = aide_dir().join(CONFIG_FILE);
    if !path.exists() {
        return Ok(Config::default());
    }
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("Failed to read {}", path.display()))?;
    toml::from_str(&content).with_context(|| format!("Failed to parse {}", path.display()))
}

/// Save config to ~/.aide/config.toml.
pub fn save(config: &Config) -> Result<()> {
    let dir = aide_dir();
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(CONFIG_FILE);
    let content = toml::to_string_pretty(config)?;
    std::fs::write(&path, content)?;
    Ok(())
}

/// Register a new agent directory.
pub fn register(name: &str, path: &Path) -> Result<()> {
    let mut config = load()?;

    // Check for duplicates
    let path_str = path.to_string_lossy().to_string();
    if config.agents.iter().any(|a| a.name == name) {
        anyhow::bail!("Agent '{}' already registered", name);
    }

    config.agents.push(AgentEntry {
        name: name.to_string(),
        path: path_str,
    });
    save(&config)
}

/// Unregister an agent by name.
pub fn unregister(name: &str) -> Result<()> {
    let mut config = load()?;
    let before = config.agents.len();
    config.agents.retain(|a| a.name != name);
    if config.agents.len() == before {
        anyhow::bail!("Agent '{}' not found", name);
    }
    save(&config)
}

/// Resolve agent name or path to a directory.
pub fn resolve(name_or_path: &str) -> Result<PathBuf> {
    // First try as a registered name
    let config = load()?;
    if let Some(entry) = config.agents.iter().find(|a| a.name == name_or_path) {
        let expanded = shellexpand::tilde(&entry.path).to_string();
        return Ok(PathBuf::from(expanded));
    }

    // Then try as a path
    let path = PathBuf::from(shellexpand::tilde(name_or_path).as_ref());
    if path.exists() && crate::aidefile::exists(&path) {
        return Ok(path);
    }

    anyhow::bail!(
        "'{}' is not a registered agent name or a directory with an Aidefile",
        name_or_path
    )
}

/// List all registered agents.
pub fn list() -> Result<Vec<AgentEntry>> {
    Ok(load()?.agents)
}
