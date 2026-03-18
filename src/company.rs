use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

/// Company specification — defines a team of AI agents with org structure.
///
/// ```toml
/// [company]
/// name = "My Startup"
///
/// [founder]
/// name = "ydwu"
///
/// [agents.ceo]
/// image = "chatfounder/ceo"
/// reports_to = "founder"
/// channels = ["general", "strategy"]
///
/// [agents.dev]
/// image = "chatfounder/dev"
/// reports_to = "ceo"
/// channels = ["dev", "general"]
/// ```
#[derive(Debug, Deserialize)]
pub struct CompanySpec {
    pub company: CompanyInfo,
    #[serde(default)]
    pub founder: Option<FounderInfo>,
    #[serde(default)]
    pub agents: HashMap<String, AgentConfig>,
}

#[derive(Debug, Deserialize)]
pub struct CompanyInfo {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct FounderInfo {
    pub name: String,
    #[serde(default)]
    pub email: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AgentConfig {
    pub image: String,
    #[serde(default)]
    pub reports_to: Option<String>,
    #[serde(default)]
    pub channels: Vec<String>,
}

impl CompanySpec {
    pub fn load(path: &Path) -> Result<Self> {
        let file_path = if path.is_dir() {
            path.join("company.toml")
        } else {
            path.to_path_buf()
        };
        let content = std::fs::read_to_string(&file_path)
            .with_context(|| format!("failed to read {}", file_path.display()))?;
        toml::from_str(&content)
            .with_context(|| format!("failed to parse {}", file_path.display()))
    }
}
