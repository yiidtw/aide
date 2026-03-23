use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use crate::config::AgentDef;

/// Persistent instance state stored on disk.
///
/// Each instance lives at `~/.aide/instances/<name>/cognition/instance.toml`.
/// This manifest captures the instance's identity, its parent agent type,
/// and any scheduled cron entries. It is created by [`InstanceManager::spawn()`]
/// and updated whenever cron entries are added or removed.
///
/// ## On-disk layout
///
/// ```text
/// ~/.aide/instances/<name>/
///   occupation/          # shareable job definition
///     Agentfile.toml     # agent manifest
///     persona.md         # copied from agent type definition
///     skills/            # skill scripts
///     knowledge/         # knowledge files (from agent image)
///   cognition/           # instance-specific brain
///     instance.toml      # this manifest
///     memory/            # persistent memory across runs
///     logs/              # daily log files (YYYY-MM-DD.log)
/// ```
#[derive(Debug, Serialize, Deserialize)]
pub struct InstanceManifest {
    /// Instance name (e.g. `"school-assistant.ydwu"`).
    pub name: String,
    /// The agent type this instance was spawned from (e.g. `"school-assistant"`).
    pub agent_type: String,
    /// UTC timestamp of when this instance was created.
    pub created_at: DateTime<Utc>,
    /// Contact email associated with this instance.
    pub email: String,
    /// Role description (e.g. `"University course assistant"`).
    pub role: String,
    /// Domain scopes this instance operates in (e.g. `["education", "email"]`).
    pub domains: Vec<String>,
    /// Scheduled cron entries for this instance. Managed via
    /// `aide.sh cron add/rm` commands.
    #[serde(default)]
    pub cron: Vec<CronEntry>,
    /// GitHub repo for issue-driven workflow (e.g. `"yiidtw/debate-agent"`).
    /// Set by `aide deploy --github`. Used by the daemon to poll for new issues.
    #[serde(default)]
    pub github_repo: Option<String>,
    /// Globally unique instance ID (UUIDv4), generated at spawn time.
    #[serde(default)]
    pub uuid: Option<String>,
    /// Machine hostname where this instance runs.
    #[serde(default)]
    pub machine_id: Option<String>,
}

/// A scheduled skill execution entry.
///
/// Stored within [`InstanceManifest::cron`]. The daemon (`aide.sh up`)
/// evaluates these entries and runs the corresponding skill when the
/// cron schedule matches.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronEntry {
    /// Cron expression in standard 5-field format (e.g. `"0 8 * * *"`).
    pub schedule: String,
    /// Name of the skill to execute (must match a key in `[skills.*]`).
    pub skill: String,
    /// UTC timestamp of the last successful run, if any.
    /// Updated by the daemon after each execution.
    #[serde(default)]
    pub last_run: Option<DateTime<Utc>>,
}

/// Runtime view of an instance, used by `aide.sh ps`.
///
/// This is a read-only projection of [`InstanceManifest`] enriched with
/// runtime information (status, last activity). Built by [`InstanceManager::list()`].
#[derive(Debug)]
pub struct InstanceInfo {
    /// Instance name.
    pub name: String,
    /// Parent agent type name.
    pub agent_type: String,
    /// Current runtime status (active or stopped).
    pub status: InstanceStatus,
    /// When the instance was created.
    #[allow(dead_code)]
    pub created_at: DateTime<Utc>,
    /// Contact email.
    pub email: String,
    /// Role description.
    pub role: String,
    /// Number of cron entries registered.
    pub cron_count: usize,
    /// Most recent log line, if any. Used for the "last activity" column in `aide.sh ps`.
    pub last_activity: Option<String>,
}

/// Runtime status of an agent instance.
///
/// Currently determined by the presence of a PID file (TODO).
/// Displayed in `aide.sh ps` output.
#[derive(Debug, PartialEq)]
pub enum InstanceStatus {
    /// The instance daemon is running (or presumed running).
    Active,
    /// The instance exists on disk but no daemon process is active.
    #[allow(dead_code)]
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

/// Manages agent instances on disk.
///
/// Instances are stored under `~/.aide/instances/<name>/`. This manager
/// provides CRUD operations for instances, cron management, and log access.
///
/// The Docker analogy: if agent types are images, instances are containers.
/// `InstanceManager` is the container runtime.
pub struct InstanceManager {
    /// Root directory for all instances (typically `~/.aide/instances/`).
    base_dir: PathBuf,
}

impl InstanceManager {
    /// Create a new instance manager rooted at the parent of `data_dir`.
    ///
    /// Given a data dir like `"~/.aide/data"`, the instances directory becomes
    /// `~/.aide/instances/`. Tilde expansion is performed automatically.
    pub fn new(data_dir: &str) -> Self {
        let expanded = shellexpand::tilde(data_dir).to_string();
        // instances live alongside data_dir: ~/.aide/instances/
        let base = Path::new(&expanded)
            .parent()
            .unwrap_or(Path::new(&expanded))
            .join("instances");
        Self { base_dir: base }
    }

    /// Returns the base directory path where all instances are stored.
    pub fn base_dir(&self) -> &Path {
        &self.base_dir
    }

    /// Spawn a new instance from an agent type definition.
    ///
    /// Creates the instance directory structure with occupation/ and cognition/
    /// subdirectories, copies the persona file if one exists in the agent
    /// definition, and writes the initial `cognition/instance.toml` manifest.
    ///
    /// # Errors
    ///
    /// Returns an error if an instance with the same name already exists.
    /// Use `aide.sh rm <name>` to remove it first.
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

        // Create directory structure (occupation/ + cognition/ split)
        fs::create_dir_all(inst_dir.join("cognition/memory"))?;
        fs::create_dir_all(inst_dir.join("cognition/logs"))?;

        let manifest = InstanceManifest {
            name: instance_name.to_string(),
            agent_type: agent_type.to_string(),
            created_at: Utc::now(),
            email: def.email.clone(),
            role: def.role.clone(),
            domains: def.domains.clone(),
            cron: Vec::new(),
            github_repo: None,
            uuid: Some(uuid::Uuid::new_v4().to_string()),
            machine_id: Some(gethostname()),
        };

        // Write persona.md to occupation/persona.md if agent type has one
        if let Some(persona_path) = &def.persona_path {
            let expanded = shellexpand::tilde(persona_path).to_string();
            if Path::new(&expanded).exists() {
                fs::create_dir_all(inst_dir.join("occupation"))?;
                fs::copy(&expanded, inst_dir.join("occupation/persona.md"))
                    .context("failed to copy persona.md")?;
            }
        }

        self.save_manifest(instance_name, &manifest)?;
        Ok(manifest)
    }

    /// Remove an instance from disk.
    ///
    /// If `keep_memory` is true, the `memory/` subdirectory is backed up
    /// to `.<name>.memory.bak` before deletion (allowing later recovery).
    ///
    /// Returns `Ok(true)` if the instance was found and removed,
    /// `Ok(false)` if the instance directory did not exist.
    pub fn remove(&self, name: &str, keep_memory: bool) -> Result<bool> {
        let inst_dir = self.base_dir.join(name);
        if !inst_dir.exists() {
            return Ok(false);
        }

        if keep_memory {
            // Move memory dir to a backup location
            let backup = self.base_dir.join(format!(".{}.memory.bak", name));
            // Try new path first, fall back to old
            let mem_dir = resolve_path(&inst_dir, "cognition/memory", "memory");
            if mem_dir.exists() {
                fs::rename(&mem_dir, &backup).ok();
            }
        }

        fs::remove_dir_all(&inst_dir)?;
        Ok(true)
    }

    /// List all instances, sorted alphabetically by name.
    ///
    /// Scans the instances base directory for subdirectories containing
    /// `instance.toml`. Hidden directories (starting with `.`) are skipped.
    /// Each valid instance is loaded and enriched with its last log entry.
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

            // Check for instance.toml in cognition/ (new) or root (old)
            let new_manifest = entry.path().join("cognition/instance.toml");
            let old_manifest = entry.path().join("instance.toml");
            if !new_manifest.exists() && !old_manifest.exists() {
                continue;
            }

            match self.load_manifest(&name) {
                Ok(manifest) => {
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
                Err(e) => {
                    eprintln!("warning: skipping instance '{}': {}", name, e);
                }
            }
        }

        instances.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(instances)
    }

    /// Get a specific instance by name.
    ///
    /// Returns `Ok(None)` if the instance directory does not exist.
    /// Returns `Err` if the directory exists but the manifest is unreadable.
    pub fn get(&self, name: &str) -> Result<Option<InstanceManifest>> {
        let inst_dir = self.base_dir.join(name);
        if !inst_dir.exists() {
            return Ok(None);
        }
        Ok(Some(self.load_manifest(name)?))
    }

    /// Add a cron entry to an instance.
    ///
    /// Persists a new [`CronEntry`] to the instance manifest. Duplicate
    /// skill names are rejected (one cron entry per skill).
    ///
    /// # Errors
    ///
    /// - Instance not found.
    /// - A cron entry for the given skill already exists.
    pub fn cron_add(&self, name: &str, schedule: &str, skill: &str) -> Result<()> {
        let mut manifest = self
            .load_manifest(name)
            .with_context(|| format!("instance '{}' not found", name))?;

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

    /// Remove a cron entry for a given skill.
    ///
    /// Returns `Ok(true)` if an entry was found and removed, `Ok(false)` if
    /// no entry matched the given skill name.
    pub fn cron_rm(&self, name: &str, skill: &str) -> Result<bool> {
        let mut manifest = self
            .load_manifest(name)
            .with_context(|| format!("instance '{}' not found", name))?;

        let before = manifest.cron.len();
        manifest.cron.retain(|c| c.skill != skill);
        let removed = manifest.cron.len() < before;

        if removed {
            self.save_manifest(name, &manifest)?;
        }
        Ok(removed)
    }

    /// Update the `last_run` timestamp of a cron entry to `Utc::now()`.
    ///
    /// Finds the cron entry matching `skill` and sets its `last_run` field.
    /// The updated manifest is written back to disk.
    ///
    /// # Errors
    ///
    /// - Instance not found.
    /// - No cron entry matches the given skill name.
    pub fn cron_update_last_run(&self, name: &str, skill: &str) -> Result<()> {
        let mut manifest = self
            .load_manifest(name)
            .with_context(|| format!("instance '{}' not found", name))?;

        let entry = manifest
            .cron
            .iter_mut()
            .find(|c| c.skill == skill)
            .with_context(|| format!("no cron entry for skill '{}'", skill))?;

        entry.last_run = Some(Utc::now());
        self.save_manifest(name, &manifest)?;
        Ok(())
    }

    /// List all cron entries for an instance.
    ///
    /// Returns the cron entries from the instance manifest.
    /// Errors if the instance does not exist.
    pub fn cron_list(&self, name: &str) -> Result<Vec<CronEntry>> {
        let manifest = self
            .load_manifest(name)
            .with_context(|| format!("instance '{}' not found", name))?;
        Ok(manifest.cron)
    }

    /// Resolve the logs directory for an instance.
    /// Uses `cognition/logs/` if it exists, falls back to `logs/` for backward compat.
    fn logs_dir(&self, name: &str) -> PathBuf {
        resolve_path(&self.base_dir.join(name), "cognition/logs", "logs")
    }

    /// Returns the path to the current daily log file for an instance.
    ///
    /// The path is `<instance>/cognition/logs/YYYY-MM-DD.log` based on today's UTC date.
    /// Falls back to `<instance>/logs/` for backward compat.
    /// Note: the file (and parent directory) may not exist yet.
    pub fn log_path(&self, name: &str) -> PathBuf {
        let today = Utc::now().format("%Y-%m-%d").to_string();
        self.logs_dir(name).join(format!("{}.log", today))
    }

    /// Append a timestamped log entry to the instance's daily log file.
    ///
    /// Logs are stored at `<instance>/cognition/logs/YYYY-MM-DD.log` with lines
    /// formatted as `[HH:MM:SS] <entry>`. The log directory is created
    /// automatically if it does not exist.
    ///
    /// Before appending, the log file is rotated if it exceeds 1 MB.
    pub fn append_log(&self, name: &str, entry: &str) -> Result<()> {
        let log_dir = self.logs_dir(name);
        fs::create_dir_all(&log_dir)?;

        let log_file = self.log_path(name);

        self.maybe_rotate_log(&log_file)?;

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

    /// Read the most recent log entries for an instance.
    ///
    /// Reads from the newest log files first, collecting up to `lines` entries
    /// in reverse chronological order, then returns them in chronological order.
    /// Returns an empty vec if the log directory does not exist.
    pub fn read_logs(&self, name: &str, lines: usize) -> Result<Vec<String>> {
        let log_dir = self.logs_dir(name);
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

    /// Rotate a log file if it exceeds 1 MB.
    ///
    /// Keeps at most 2 rotated copies (`.log.1` and `.log.2`).
    /// The oldest rotated file is deleted to make room.
    fn maybe_rotate_log(&self, log_file: &Path) -> Result<()> {
        if let Ok(meta) = fs::metadata(log_file) {
            if meta.len() > 1_048_576 {
                // 1 MB
                let rotated_1 = log_file.with_extension("log.1");
                let rotated_2 = log_file.with_extension("log.2");
                if rotated_2.exists() {
                    fs::remove_file(&rotated_2)?;
                }
                if rotated_1.exists() {
                    fs::rename(&rotated_1, &rotated_2)?;
                }
                fs::rename(log_file, &rotated_1)?;
            }
        }
        Ok(())
    }

    /// Ensure an instance has a UUID, backfilling one if missing.
    /// Returns the UUID string.
    pub fn ensure_uuid(&self, name: &str) -> Result<String> {
        let mut manifest = self.load_manifest(name)?;
        if manifest.uuid.is_none() {
            manifest.uuid = Some(uuid::Uuid::new_v4().to_string());
            manifest.machine_id = Some(gethostname());
            self.save_manifest(name, &manifest)?;
        }
        Ok(manifest.uuid.unwrap())
    }

    fn save_manifest(&self, name: &str, manifest: &InstanceManifest) -> Result<()> {
        // Save to cognition/instance.toml (new path), fall back to root if cognition/ doesn't exist
        let inst_dir = self.base_dir.join(name);
        let cognition_dir = inst_dir.join("cognition");
        let path = if cognition_dir.exists() {
            cognition_dir.join("instance.toml")
        } else {
            inst_dir.join("instance.toml")
        };
        let content = toml::to_string_pretty(manifest)?;
        fs::write(&path, content)?;
        Ok(())
    }

    fn load_manifest(&self, name: &str) -> Result<InstanceManifest> {
        // Try cognition/instance.toml first, fall back to root instance.toml
        let inst_dir = self.base_dir.join(name);
        let new_path = inst_dir.join("cognition/instance.toml");
        let path = if new_path.exists() { new_path } else { inst_dir.join("instance.toml") };
        let content =
            fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
        let manifest: InstanceManifest =
            toml::from_str(&content).with_context(|| format!("failed to parse {}", path.display()))?;
        Ok(manifest)
    }

    fn last_log_entry(&self, name: &str) -> Option<String> {
        self.read_logs(name, 1).ok()?.into_iter().next()
    }
}

/// Resolve a path with backward compatibility.
/// Tries the new path first, falls back to the old path if the new one doesn't exist.
pub fn resolve_path(inst_dir: &Path, new: &str, old: &str) -> PathBuf {
    let new_path = inst_dir.join(new);
    if new_path.exists() { new_path } else { inst_dir.join(old) }
}

/// Get the machine hostname.
fn gethostname() -> String {
    hostname::get()
        .map(|h| h.to_string_lossy().to_string())
        .unwrap_or_else(|_| "unknown".to_string())
}

/// Return the first 4 characters of a UUID as a short prefix.
pub fn uuid_prefix(uuid: &str) -> String {
    uuid.chars().take(4).collect()
}

/// Derive the default instance name from agent type and system username.
///
/// Format: `<agent_type>.<username>` (e.g. `"school-assistant.ydwu"`).
/// Falls back to `"anon"` if neither `$USER` nor `$USERNAME` is set.
pub fn default_instance_name(agent_type: &str) -> String {
    let user = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "anon".to_string());
    format!("{}.{}", agent_type, user)
}
