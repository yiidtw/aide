//! Aidefile parser — the single file that turns a Claude project into an agent.
//!
//! Every field is optional except `[persona].name`.
//! Missing sections get safe defaults.

use serde::Deserialize;
use std::path::Path;

/// Parsed Aidefile.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Aidefile {
    #[serde(default)]
    pub persona: Persona,
    #[serde(default)]
    pub budget: Budget,
    #[serde(default)]
    pub memory: Memory,
    #[serde(default)]
    pub hooks: Hooks,
    #[serde(default)]
    pub skills: Skills,
    #[serde(default)]
    pub trigger: Trigger,
    #[serde(default)]
    pub vault: Vault,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Persona {
    pub name: String,
    #[serde(default)]
    pub style: Option<String>,
}

impl Default for Persona {
    fn default() -> Self {
        Self {
            name: "unnamed".into(),
            style: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Budget {
    /// Token budget as string like "100k", "1m", "500000".
    #[serde(default = "Budget::default_tokens")]
    pub tokens: String,
    /// Max re-invocations if task is incomplete.
    #[serde(default = "Budget::default_max_retries")]
    pub max_retries: u32,
    /// Timeout per invocation as string like "30s", "5m", "1h".
    #[serde(default)]
    pub timeout: Option<String>,
}

impl Budget {
    fn default_tokens() -> String {
        "200k".into()
    }
    fn default_max_retries() -> u32 {
        3
    }

    /// Parse token string to u64. Supports "100k", "1m", "500000".
    pub fn tokens_limit(&self) -> u64 {
        parse_token_str(&self.tokens)
    }

    /// Parse timeout string to Duration. Supports "30s", "5m", "1h".
    pub fn timeout_duration(&self) -> Option<std::time::Duration> {
        self.timeout.as_deref().map(parse_duration)
    }
}

impl Default for Budget {
    fn default() -> Self {
        Self {
            tokens: Self::default_tokens(),
            max_retries: Self::default_max_retries(),
            timeout: None,
        }
    }
}

/// Parse duration strings: "30s" → 30s, "5m" → 300s, "1h" → 3600s.
pub fn parse_duration(s: &str) -> std::time::Duration {
    let s = s.trim().to_lowercase();
    let secs = if let Some(n) = s.strip_suffix('s') {
        n.parse::<u64>().unwrap_or(300)
    } else if let Some(n) = s.strip_suffix('m') {
        n.parse::<u64>().unwrap_or(5) * 60
    } else if let Some(n) = s.strip_suffix('h') {
        n.parse::<u64>().unwrap_or(1) * 3600
    } else {
        s.parse::<u64>().unwrap_or(300)
    };
    std::time::Duration::from_secs(secs)
}

#[derive(Debug, Clone, Deserialize)]
pub struct Memory {
    /// Auto-compact threshold as token string.
    #[serde(default = "Memory::default_compact_after")]
    pub compact_after: String,
}

impl Memory {
    fn default_compact_after() -> String {
        "200k".into()
    }

    pub fn compact_threshold(&self) -> u64 {
        parse_token_str(&self.compact_after)
    }
}

impl Default for Memory {
    fn default() -> Self {
        Self {
            compact_after: Self::default_compact_after(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Hooks {
    #[serde(default)]
    pub on_spawn: Vec<String>,
    #[serde(default)]
    pub on_complete: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Skills {
    #[serde(default)]
    pub include: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Trigger {
    /// "manual" | "issue" | "cron:EXPR" | "webhook:URL"
    #[serde(default = "Trigger::default_on")]
    pub on: String,
}

impl Trigger {
    fn default_on() -> String {
        "manual".into()
    }

    pub fn is_manual(&self) -> bool {
        self.on == "manual"
    }

    pub fn is_issue(&self) -> bool {
        self.on == "issue"
    }

    pub fn cron_expr(&self) -> Option<&str> {
        self.on.strip_prefix("cron:")
    }

    pub fn webhook_url(&self) -> Option<&str> {
        self.on.strip_prefix("webhook:")
    }
}

impl Default for Trigger {
    fn default() -> Self {
        Self {
            on: Self::default_on(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Vault {
    /// Required secret keys from vault.
    #[serde(default)]
    pub keys: Vec<String>,
}

/// Parse token strings: "100k" → 100_000, "1m" → 1_000_000, "500000" → 500_000.
pub fn parse_token_str(s: &str) -> u64 {
    let s = s.trim().to_lowercase();
    if let Some(n) = s.strip_suffix('k') {
        n.parse::<u64>().unwrap_or(200) * 1_000
    } else if let Some(n) = s.strip_suffix('m') {
        n.parse::<u64>().unwrap_or(1) * 1_000_000
    } else {
        s.parse::<u64>().unwrap_or(200_000)
    }
}

/// Find and parse Aidefile in a directory.
pub fn load(dir: &Path) -> anyhow::Result<Aidefile> {
    let path = dir.join("Aidefile");
    if !path.exists() {
        anyhow::bail!("No Aidefile found in {}", dir.display());
    }
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("Failed to read {}", path.display()))?;
    let aidefile: Aidefile =
        toml::from_str(&content).with_context(|| format!("Failed to parse {}", path.display()))?;
    Ok(aidefile)
}

/// Check if a directory contains an Aidefile.
pub fn exists(dir: &Path) -> bool {
    dir.join("Aidefile").exists()
}

use anyhow::Context as _;

// ── Kani proofs ──────────────────────────────────────────────────────

// Note: Kani proofs for string-heavy code (TOML parsing) are impractical due to
// unbounded string unwinding. These invariants are covered by unit tests instead.
// Kani proofs are reserved for pure-logic modules (budget, vault key filtering).

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_token_str() {
        assert_eq!(parse_token_str("100k"), 100_000);
        assert_eq!(parse_token_str("1m"), 1_000_000);
        assert_eq!(parse_token_str("500000"), 500_000);
        assert_eq!(parse_token_str("0k"), 0);
        assert_eq!(parse_token_str(""), 200_000); // fallback
    }

    #[test]
    fn test_trigger_variants() {
        let t = Trigger { on: "manual".into() };
        assert!(t.is_manual());
        assert!(!t.is_issue());
        assert!(t.cron_expr().is_none());

        let t = Trigger { on: "cron:0 3 * * *".into() };
        assert_eq!(t.cron_expr(), Some("0 3 * * *"));

        let t = Trigger { on: "issue".into() };
        assert!(t.is_issue());
    }

    #[test]
    fn test_full_aidefile_parse() {
        let content = r#"
[persona]
name = "Senior Reviewer"
style = "direct, terse"

[budget]
tokens = "100k"
max_retries = 5

[memory]
compact_after = "300k"

[hooks]
on_spawn = ["inject-vault"]
on_complete = ["commit-memory", "notify"]

[skills]
include = ["code-review", "test"]

[trigger]
on = "issue"

[vault]
keys = ["GITHUB_TOKEN"]
"#;
        let af: Aidefile = toml::from_str(content).unwrap();
        assert_eq!(af.persona.name, "Senior Reviewer");
        assert_eq!(af.budget.tokens_limit(), 100_000);
        assert_eq!(af.budget.max_retries, 5);
        assert_eq!(af.memory.compact_threshold(), 300_000);
        assert_eq!(af.hooks.on_spawn, vec!["inject-vault"]);
        assert_eq!(af.hooks.on_complete, vec!["commit-memory", "notify"]);
        assert_eq!(af.skills.include, vec!["code-review", "test"]);
        assert!(af.trigger.is_issue());
        assert_eq!(af.vault.keys, vec!["GITHUB_TOKEN"]);
    }

    #[test]
    fn test_parse_duration() {
        assert_eq!(parse_duration("30s"), std::time::Duration::from_secs(30));
        assert_eq!(parse_duration("5m"), std::time::Duration::from_secs(300));
        assert_eq!(parse_duration("1h"), std::time::Duration::from_secs(3600));
        assert_eq!(parse_duration("120"), std::time::Duration::from_secs(120));
    }

    #[test]
    fn test_budget_with_timeout() {
        let content = r#"
[persona]
name = "Test"

[budget]
tokens = "50k"
timeout = "10m"
"#;
        let af: Aidefile = toml::from_str(content).unwrap();
        assert_eq!(
            af.budget.timeout_duration(),
            Some(std::time::Duration::from_secs(600))
        );
    }

    #[test]
    fn test_budget_without_timeout() {
        let content = r#"
[persona]
name = "Test"

[budget]
tokens = "50k"
"#;
        let af: Aidefile = toml::from_str(content).unwrap();
        assert_eq!(af.budget.timeout_duration(), None);
    }
}
