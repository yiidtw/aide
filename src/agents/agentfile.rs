use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Parsed Agentfile.toml — the agent package manifest
#[derive(Debug, Deserialize)]
pub struct AgentfileSpec {
    pub agent: AgentMeta,
    #[serde(default)]
    pub persona: Option<PersonaSection>,
    #[serde(default)]
    pub skills: HashMap<String, SkillDef>,
    #[serde(default)]
    pub seed: Option<SeedSection>,
    #[serde(default)]
    pub env: Option<EnvSection>,
    #[serde(default)]
    pub soul: Option<SoulSection>,
}

#[derive(Debug, Deserialize)]
pub struct SoulSection {
    /// Preferred local model for daemon mode (e.g. "llama3.2:3b")
    #[serde(default)]
    pub prefer: Option<String>,
    /// Minimum model size (e.g. "1b")
    #[serde(default)]
    pub min_params: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AgentMeta {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PersonaSection {
    pub file: String,
}

#[derive(Debug, Deserialize)]
pub struct SkillDef {
    /// Script-based skill (path to .sh or executable)
    #[serde(default)]
    pub script: Option<String>,
    /// Prompt-based skill (path to .md prompt file)
    #[serde(default)]
    pub prompt: Option<String>,
    /// Cron schedule (optional)
    #[serde(default)]
    pub schedule: Option<String>,
    /// Per-skill env vars (overrides agent-level [env] for this skill)
    /// If set, ONLY these vars are injected when this skill runs.
    #[serde(default)]
    pub env: Option<Vec<String>>,
    /// Human-readable description of what this skill does
    #[serde(default)]
    pub description: Option<String>,
    /// Usage string for --help (e.g. "lms [courses|assignments|grades]")
    #[serde(default)]
    pub usage: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SeedSection {
    pub dir: String,
}

#[derive(Debug, Deserialize)]
pub struct EnvSection {
    #[serde(default)]
    pub required: Vec<String>,
    #[serde(default)]
    pub optional: Vec<String>,
}

impl AgentfileSpec {
    /// Load and parse Agentfile.toml from the given directory
    pub fn load(dir: &Path) -> Result<Self> {
        let path = dir.join("Agentfile.toml");
        if !path.exists() {
            bail!("Agentfile.toml not found in {}", dir.display());
        }
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let spec: AgentfileSpec = toml::from_str(&content)
            .with_context(|| format!("failed to parse {}", path.display()))?;
        Ok(spec)
    }

    /// Validate that all referenced files exist relative to the given directory
    pub fn validate(&self, dir: &Path) -> Result<Vec<String>> {
        let mut warnings = Vec::new();

        // Check persona file
        if let Some(persona) = &self.persona {
            let persona_path = dir.join(&persona.file);
            if !persona_path.exists() {
                bail!(
                    "persona file not found: {} (expected at {})",
                    persona.file,
                    persona_path.display()
                );
            }
        }

        // Check skill files
        for (name, skill) in &self.skills {
            if skill.script.is_none() && skill.prompt.is_none() {
                bail!(
                    "skill '{}' must have either 'script' or 'prompt' field",
                    name
                );
            }
            if skill.script.is_some() && skill.prompt.is_some() {
                warnings.push(format!(
                    "skill '{}' has both script and prompt — script takes precedence",
                    name
                ));
            }
            if let Some(script) = &skill.script {
                let script_path = dir.join(script);
                if !script_path.exists() {
                    bail!(
                        "skill '{}' script not found: {} (expected at {})",
                        name,
                        script,
                        script_path.display()
                    );
                }
            }
            if let Some(prompt) = &skill.prompt {
                let prompt_path = dir.join(prompt);
                if !prompt_path.exists() {
                    bail!(
                        "skill '{}' prompt not found: {} (expected at {})",
                        name,
                        prompt,
                        prompt_path.display()
                    );
                }
            }
        }

        // Check seed dir
        if let Some(seed) = &self.seed {
            let seed_path = dir.join(&seed.dir);
            if !seed_path.exists() {
                warnings.push(format!(
                    "seed directory not found: {} — will be ignored",
                    seed.dir
                ));
            }
        }

        Ok(warnings)
    }

    /// Generate --help output for `aide.sh exec <instance> --help`
    pub fn format_help(&self, instance_name: &str) -> String {
        let mut out = String::new();

        // Header
        out.push_str(&format!(
            "{} ({}:{})\n",
            instance_name,
            self.agent.name,
            self.agent.version
        ));
        if let Some(desc) = &self.agent.description {
            out.push_str(&format!("  {}\n", desc));
        }

        // Skills
        if !self.skills.is_empty() {
            out.push_str("\nSkills:\n");

            let mut names: Vec<&String> = self.skills.keys().collect();
            names.sort();

            for name in names {
                let skill = &self.skills[name];

                // Usage line or just name
                let usage = skill
                    .usage
                    .as_deref()
                    .unwrap_or(name.as_str());
                out.push_str(&format!("  {}\n", usage));

                // Description
                if let Some(desc) = &skill.description {
                    out.push_str(&format!("      {}\n", desc));
                }

                // Env vars
                if let Some(env) = &skill.env {
                    if !env.is_empty() {
                        out.push_str(&format!("      env: {}\n", env.join(", ")));
                    }
                }

                out.push('\n');
            }
        }

        // Semantic mode hint
        out.push_str("Semantic mode:\n");
        out.push_str(&format!(
            "  aide.sh exec -p {} \"<natural language query>\"\n",
            instance_name
        ));
        out.push_str("  (requires LLM runtime — ollama or MCP caller)\n");

        out
    }

    /// Full archive name: <name>-<version>.tar.gz
    pub fn archive_name(&self) -> String {
        format!("{}-{}.tar.gz", self.agent.name, self.agent.version)
    }

    /// Collect all files that should be included in the build archive
    pub fn collect_files(&self, dir: &Path) -> Result<Vec<PathBuf>> {
        let mut files = Vec::new();

        // Always include Agentfile.toml
        files.push(dir.join("Agentfile.toml"));

        // Persona
        if let Some(persona) = &self.persona {
            files.push(dir.join(&persona.file));
        }

        // Skill files
        for skill in self.skills.values() {
            if let Some(script) = &skill.script {
                files.push(dir.join(script));
            }
            if let Some(prompt) = &skill.prompt {
                files.push(dir.join(prompt));
            }
        }

        // Seed directory — collect all files recursively
        if let Some(seed) = &self.seed {
            let seed_path = dir.join(&seed.dir);
            if seed_path.exists() {
                collect_dir_recursive(&seed_path, &mut files)?;
            }
        }

        Ok(files)
    }
}

/// Recursively collect all files in a directory
fn collect_dir_recursive(dir: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_dir_recursive(&path, files)?;
        } else {
            files.push(path);
        }
    }
    Ok(())
}
