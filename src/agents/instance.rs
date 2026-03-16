use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use crate::config::AgentDef;

/// Instance state on disk (~/.aide/instances/<name>/instance.toml)
#[derive(Debug, Serialize, Deserialize)]
pub struct InstanceManifest {
    pub name: String,
    pub agent_type: String,
    pub created_at: DateTime<Utc>,
    pub email: String,
    pub role: String,
    pub domains: Vec<String>,
    #[serde(default)]
    pub cron: Vec<CronEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronEntry {
    pub schedule: String,
    pub skill: String,
    #[serde(default)]
    pub last_run: Option<DateTime<Utc>>,
}

/// Runtime view of an instance (for aide ps)
#[derive(Debug)]
pub struct InstanceInfo {
    pub name: String,
    pub agent_type: String,
    pub status: InstanceStatus,
    pub created_at: DateTime<Utc>,
    pub email: String,
    pub role: String,
    pub cron_count: usize,
    pub last_activity: Option<String>,
}

#[derive(Debug, PartialEq)]
pub enum InstanceStatus {
    Active,
    Stopped,
}

impl std::fmt::Display for InstanceStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InstanceStatus::Active => write!(f, "active"),
            InstanceStatus::Stopped => write!(f, "stopped"),
        }
    }
}

/// Manages agent instances on disk
pub struct InstanceManager {
    base_dir: PathBuf,
}

impl InstanceManager {
    pub fn new(data_dir: &str) -> Self {
        let expanded = shellexpand::tilde(data_dir).to_string();
        // instances live alongside data_dir: ~/.aide/instances/
        let base = Path::new(&expanded)
            .parent()
            .unwrap_or(Path::new(&expanded))
            .join("instances");
        Self { base_dir: base }
    }

    pub fn base_dir(&self) -> &Path {
        &self.base_dir
    }

    /// Spawn a new instance from an agent type definition
    pub fn spawn(
        &self,
        agent_type: &str,
        instance_name: &str,
        def: &AgentDef,
    ) -> Result<InstanceManifest> {
        let inst_dir = self.base_dir.join(instance_name);
        if inst_dir.exists() {
            bail!(
                "instance '{}' already exists. Use `aide rm {}` first.",
                instance_name,
                instance_name
            );
        }

        // Create directory structure
        fs::create_dir_all(inst_dir.join("memory"))?;
        fs::create_dir_all(inst_dir.join("logs"))?;

        let manifest = InstanceManifest {
            name: instance_name.to_string(),
            agent_type: agent_type.to_string(),
            created_at: Utc::now(),
            email: def.email.clone(),
            role: def.role.clone(),
            domains: def.domains.clone(),
            cron: Vec::new(),
        };

        // Write persona.md stub if agent type has one
        if let Some(persona_path) = &def.persona_path {
            let expanded = shellexpand::tilde(persona_path).to_string();
            if Path::new(&expanded).exists() {
                fs::copy(&expanded, inst_dir.join("persona.md"))
                    .context("failed to copy persona.md")?;
            }
        }

        self.save_manifest(instance_name, &manifest)?;
        Ok(manifest)
    }

    /// Remove an instance (optionally keeping memory)
    pub fn remove(&self, name: &str, keep_memory: bool) -> Result<bool> {
        let inst_dir = self.base_dir.join(name);
        if !inst_dir.exists() {
            return Ok(false);
        }

        if keep_memory {
            // Move memory dir to a backup location
            let backup = self.base_dir.join(format!(".{}.memory.bak", name));
            let mem_dir = inst_dir.join("memory");
            if mem_dir.exists() {
                fs::rename(&mem_dir, &backup).ok();
            }
        }

        fs::remove_dir_all(&inst_dir)?;
        Ok(true)
    }

    /// List all instances
    pub fn list(&self) -> Result<Vec<InstanceInfo>> {
        let mut instances = Vec::new();

        if !self.base_dir.exists() {
            return Ok(instances);
        }

        for entry in fs::read_dir(&self.base_dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }

            let manifest_path = entry.path().join("instance.toml");
            if !manifest_path.exists() {
                continue;
            }

            if let Ok(manifest) = self.load_manifest(&name) {
                let last_activity = self.last_log_entry(&name);
                instances.push(InstanceInfo {
                    name: manifest.name,
                    agent_type: manifest.agent_type,
                    status: InstanceStatus::Active, // TODO: check PID file
                    created_at: manifest.created_at,
                    email: manifest.email,
                    role: manifest.role,
                    cron_count: manifest.cron.len(),
                    last_activity,
                });
            }
        }

        instances.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(instances)
    }

    /// Get a specific instance
    pub fn get(&self, name: &str) -> Result<Option<InstanceManifest>> {
        let inst_dir = self.base_dir.join(name);
        if !inst_dir.exists() {
            return Ok(None);
        }
        Ok(Some(self.load_manifest(name)?))
    }

    /// Add a cron entry to an instance
    pub fn cron_add(&self, name: &str, schedule: &str, skill: &str) -> Result<()> {
        let mut manifest = self
            .load_manifest(name)
            .context(format!("instance '{}' not found", name))?;

        // Check for duplicate
        if manifest.cron.iter().any(|c| c.skill == skill) {
            bail!("cron entry for skill '{}' already exists", skill);
        }

        manifest.cron.push(CronEntry {
            schedule: schedule.to_string(),
            skill: skill.to_string(),
            last_run: None,
        });

        self.save_manifest(name, &manifest)?;
        Ok(())
    }

    /// Remove a cron entry
    pub fn cron_rm(&self, name: &str, skill: &str) -> Result<bool> {
        let mut manifest = self
            .load_manifest(name)
            .context(format!("instance '{}' not found", name))?;

        let before = manifest.cron.len();
        manifest.cron.retain(|c| c.skill != skill);
        let removed = manifest.cron.len() < before;

        if removed {
            self.save_manifest(name, &manifest)?;
        }
        Ok(removed)
    }

    /// List cron entries for an instance
    pub fn cron_list(&self, name: &str) -> Result<Vec<CronEntry>> {
        let manifest = self
            .load_manifest(name)
            .context(format!("instance '{}' not found", name))?;
        Ok(manifest.cron)
    }

    /// Append a log entry
    pub fn append_log(&self, name: &str, entry: &str) -> Result<()> {
        let log_dir = self.base_dir.join(name).join("logs");
        fs::create_dir_all(&log_dir)?;

        let today = Utc::now().format("%Y-%m-%d").to_string();
        let log_file = log_dir.join(format!("{}.log", today));

        let timestamp = Utc::now().format("%H:%M:%S").to_string();
        let line = format!("[{}] {}\n", timestamp, entry);

        use std::io::Write;
        let mut f = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_file)?;
        f.write_all(line.as_bytes())?;
        Ok(())
    }

    /// Read recent log entries
    pub fn read_logs(&self, name: &str, lines: usize) -> Result<Vec<String>> {
        let log_dir = self.base_dir.join(name).join("logs");
        if !log_dir.exists() {
            return Ok(Vec::new());
        }

        // Find most recent log files
        let mut log_files: Vec<PathBuf> = fs::read_dir(&log_dir)?
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path()
                    .extension()
                    .map(|ext| ext == "log")
                    .unwrap_or(false)
            })
            .map(|e| e.path())
            .collect();
        log_files.sort();
        log_files.reverse();

        let mut result = Vec::new();
        for log_file in log_files {
            let content = fs::read_to_string(&log_file)?;
            for line in content.lines().rev() {
                result.push(line.to_string());
                if result.len() >= lines {
                    break;
                }
            }
            if result.len() >= lines {
                break;
            }
        }

        result.reverse();
        Ok(result)
    }

    fn save_manifest(&self, name: &str, manifest: &InstanceManifest) -> Result<()> {
        let path = self.base_dir.join(name).join("instance.toml");
        let content = toml::to_string_pretty(manifest)?;
        fs::write(&path, content)?;
        Ok(())
    }

    fn load_manifest(&self, name: &str) -> Result<InstanceManifest> {
        let path = self.base_dir.join(name).join("instance.toml");
        let content =
            fs::read_to_string(&path).context(format!("failed to read {}", path.display()))?;
        let manifest: InstanceManifest =
            toml::from_str(&content).context(format!("failed to parse {}", path.display()))?;
        Ok(manifest)
    }

    fn last_log_entry(&self, name: &str) -> Option<String> {
        self.read_logs(name, 1).ok()?.into_iter().next()
    }
}

/// Derive default instance name from agent type + system user
pub fn default_instance_name(agent_type: &str) -> String {
    let user = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "anon".to_string());
    format!("{}.{}", agent_type, user)
}
