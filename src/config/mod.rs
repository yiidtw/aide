use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Deserialize)]
pub struct AideConfig {
    pub aide: AideMeta,
    #[serde(default)]
    pub machines: HashMap<String, Machine>,
    #[serde(default)]
    pub dispatch: HashMap<String, DispatchRule>,
    #[serde(default)]
    pub sync: SyncConfig,
    #[serde(default)]
    pub agents: HashMap<String, AgentDef>,
}

#[derive(Debug, Deserialize)]
pub struct AideMeta {
    pub name: String,
    #[serde(default = "default_data_dir")]
    pub data_dir: String,
    #[serde(default)]
    pub vault_path: Option<String>,
}

fn default_data_dir() -> String {
    "~/.aide/data".to_string()
}

#[derive(Debug, Deserialize)]
pub struct Machine {
    pub host: String,
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub always_on: bool,
}

#[derive(Debug, Deserialize)]
pub struct DispatchRule {
    pub on: String,
    #[serde(default)]
    pub prefer: Option<String>,
    #[serde(default)]
    pub fallback: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct SyncConfig {
    #[serde(default)]
    pub vault: Option<SyncVault>,
    #[serde(default)]
    pub skills: Option<SyncSkills>,
    #[serde(default)]
    pub memory: Option<SyncMemory>,
}

#[derive(Debug, Deserialize)]
pub struct SyncVault {
    pub method: String,
    #[serde(default = "default_trigger")]
    pub trigger: String,
    #[serde(default)]
    pub targets: Vec<String>,
}

fn default_trigger() -> String {
    "on-change".to_string()
}

#[derive(Debug, Deserialize)]
pub struct SyncSkills {
    pub method: String,
    #[serde(default)]
    pub repo: Option<String>,
    #[serde(default)]
    pub auto_pull: bool,
}

#[derive(Debug, Deserialize)]
pub struct SyncMemory {
    pub method: String,
    #[serde(default = "default_conflict")]
    pub conflict: String,
}

fn default_conflict() -> String {
    "causal".to_string()
}

#[derive(Debug, Deserialize)]
pub struct AgentDef {
    pub email: String,
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub domains: Vec<String>,
    #[serde(default)]
    pub persona_path: Option<String>,
}

impl Default for AideConfig {
    fn default() -> Self {
        Self {
            aide: AideMeta {
                name: "aide.sh".to_string(),
                data_dir: default_data_dir(),
                vault_path: None,
            },
            machines: HashMap::new(),
            dispatch: HashMap::new(),
            sync: SyncConfig::default(),
            agents: HashMap::new(),
        }
    }
}

impl AideConfig {
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read config: {}", path.display()))?;
        let config: AideConfig = toml::from_str(&content)
            .with_context(|| format!("failed to parse config: {}", path.display()))?;
        Ok(config)
    }

    pub fn this_machine(&self) -> Option<(&String, &Machine)> {
        let hostname = hostname::get().ok()?.into_string().ok()?;
        self.machines.iter().find(|(_, m)| {
            m.host == hostname || m.host == "localhost"
        })
    }

    pub fn dispatch_to(&self, task: &str) -> Option<&str> {
        self.dispatch.get(task).map(|r| r.on.as_str())
    }
}
