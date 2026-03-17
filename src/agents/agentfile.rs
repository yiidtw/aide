use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// # Agentfile.toml Specification
///
/// The agent package manifest. Defines an agent's identity, skills, and runtime configuration.
/// This is the top-level struct produced by parsing an `Agentfile.toml` file.
///
/// An Agentfile follows the Docker analogy: an agent is an image, an instance is a container.
/// The manifest declares everything needed to build, publish, and run the agent.
///
/// ## Minimal Example
///
/// ```toml
/// [agent]
/// name = "my-agent"
/// version = "0.1.0"
/// description = "A helpful agent"
/// author = "yourname"
///
/// [persona]
/// file = "persona.md"
///
/// [skills.hello]
/// script = "skills/hello.sh"
/// ```
///
/// ## Complete Example
///
/// ```toml
/// [agent]
/// name = "school-assistant"
/// version = "0.1.0"
/// description = "University course management agent"
/// author = "ydwu"
///
/// [persona]
/// file = "persona.md"
///
/// [skills.lms]
/// script = "skills/lms.sh"
/// description = "LMS scanning (Canvas/Moodle)"
/// usage = "lms [courses|assignments|grades]"
/// schedule = "0 8 * * *"
/// env = ["LMS_TOKEN"]
///
/// [skills.email]
/// script = "skills/email.sh"
/// description = "Email triage"
/// usage = "email [check|unread|send TO SUBJ]"
///
/// [seed]
/// dir = "seed/"
///
/// [env]
/// required = ["LMS_TOKEN"]
/// optional = ["SMTP_USER", "SMTP_PASS"]
///
/// [soul]
/// prefer = "llama3.2:3b"
/// ```
#[derive(Debug, Deserialize)]
pub struct AgentfileSpec {
    /// The `[agent]` table. Required. Contains name, version, description, and author.
    pub agent: AgentMeta,
    /// The `[persona]` table. Optional. Points to a markdown file that defines
    /// the agent's personality and behavioral guidelines.
    #[serde(default)]
    pub persona: Option<PersonaSection>,
    /// The `[skills.*]` tables. A map of skill name to [`SkillDef`].
    /// Each key becomes a subcommand in `aide.sh exec <instance> <skill>`.
    #[serde(default)]
    pub skills: HashMap<String, SkillDef>,
    /// The `[seed]` table. Optional. Points to a directory of initial knowledge
    /// files that are bundled into the agent archive.
    #[serde(default)]
    pub seed: Option<SeedSection>,
    /// The `[env]` table. Optional. Declares required and optional environment
    /// variables that the agent needs at runtime (injected from the vault).
    #[serde(default)]
    pub env: Option<EnvSection>,
    /// The `[soul]` table. Optional. LLM runtime hints for daemon mode.
    /// Ignored when the agent runs under an MCP caller (Claude Code, etc.).
    #[serde(default)]
    pub soul: Option<SoulSection>,
    /// The `[expose]` table. Optional. Declares external messaging channels.
    #[serde(default)]
    pub expose: Option<ExposeSection>,
}

/// External messaging channel configuration.
///
/// Allows the agent to be reached via external platforms.
/// Each channel is self-service — the user provides their own credentials.
///
/// ```toml
/// [expose]
/// telegram = { token_env = "TELEGRAM_BOT_TOKEN" }
/// ```
#[derive(Debug, Deserialize)]
pub struct ExposeSection {
    /// Telegram bot config. The agent responds to messages via Telegram Bot API.
    #[serde(default)]
    pub telegram: Option<TelegramExpose>,
}

#[derive(Debug, Deserialize)]
pub struct TelegramExpose {
    /// Environment variable name containing the Telegram bot token.
    /// Token is loaded from the vault at runtime.
    pub token_env: String,
}

/// LLM runtime hints for daemon mode.
///
/// The agent doesn't own its LLM — the caller brings it (Docker analogy:
/// container doesn't own CPU). This section provides hints for when the
/// daemon runs skills autonomously with a local LLM (e.g. ollama).
///
/// ```toml
/// [soul]
/// prefer = "llama3.2:3b"
/// min_params = "1b"
/// ```
///
/// In MCP mode (Claude Code, Codex, Gemini), this section is ignored —
/// the caller's frontier model is used instead.
#[derive(Debug, Deserialize)]
pub struct SoulSection {
    /// Preferred local model for daemon mode (e.g. `"llama3.2:3b"`).
    /// The daemon will attempt to pull this model via ollama if not already present.
    #[serde(default)]
    pub prefer: Option<String>,
    /// Minimum model size hint (e.g. `"1b"`, `"3b"`).
    /// The daemon may refuse to run if the available model is below this threshold.
    #[serde(default)]
    pub min_params: Option<String>,
}

/// Agent identity metadata.
///
/// Required in every Agentfile.toml under `[agent]`.
///
/// ## Validation Rules
/// - `name`: lowercase alphanumeric + hyphens, e.g. `"my-agent"`
/// - `version`: semver string, e.g. `"0.1.0"`
/// - `description`: should not be empty or start with `"TODO"`
/// - `author`: your hub.aide.sh username
///
/// ## Example
///
/// ```toml
/// [agent]
/// name = "school-assistant"
/// version = "0.1.0"
/// description = "University course management agent"
/// author = "ydwu"
/// ```
#[derive(Debug, Deserialize)]
pub struct AgentMeta {
    /// Agent name. Lowercase, hyphens allowed. Used as the image identifier
    /// on the registry and in archive filenames (`<name>-<version>.tar.gz`).
    pub name: String,
    /// Semantic version (e.g. `"0.1.0"`). Used for registry versioning.
    /// Follows [semver](https://semver.org/) conventions.
    pub version: String,
    /// One-line description of what this agent does.
    /// Shown in `aide.sh exec <instance> --help` and registry listings.
    #[serde(default)]
    pub description: Option<String>,
    /// Author username (matches hub.aide.sh account).
    /// Used for registry namespacing (`author/name:version`).
    #[serde(default)]
    pub author: Option<String>,
}

/// Persona configuration.
///
/// Points to a markdown file that defines the agent's personality,
/// role, and behavioral guidelines. This file is injected as the
/// system prompt when the agent runs in LLM-assisted mode.
///
/// ```toml
/// [persona]
/// file = "persona.md"
/// ```
#[derive(Debug, Deserialize)]
pub struct PersonaSection {
    /// Path to the persona markdown file, relative to the agent root.
    /// Typically `"persona.md"`. The file must exist at build time.
    pub file: String,
}

/// Skill definition within an agent.
///
/// Each skill is either script-based (`.sh`) or prompt-based (`.md`), but not both.
/// Skills are the executable units of an agent — they are what `aide.sh exec` runs.
///
/// ## Script-based skill
///
/// ```toml
/// [skills.hello]
/// script = "skills/hello.sh"
/// description = "A greeting skill"
/// usage = "hello [name]"
/// env = ["MY_API_KEY"]
/// schedule = "0 9 * * *"
/// ```
///
/// ## Prompt-based skill
///
/// ```toml
/// [skills.summarize]
/// prompt = "skills/summarize.md"
/// description = "Summarize text using LLM"
/// ```
///
/// ## Env Scoping
///
/// Per-skill `env` takes precedence over agent-level `[env]`.
/// If a skill declares `env`, ONLY those variables are injected (Docker secrets model).
#[derive(Debug, Deserialize)]
pub struct SkillDef {
    /// Path to shell script, relative to agent root (e.g. `"skills/hello.sh"`).
    /// Must be executable. Mutually exclusive with `prompt`.
    #[serde(default)]
    pub script: Option<String>,
    /// Path to prompt markdown file, relative to agent root.
    /// Used for LLM-driven skills. Mutually exclusive with `script`.
    #[serde(default)]
    pub prompt: Option<String>,
    /// Cron schedule in standard 5-field format (e.g. `"0 8 * * *"`).
    /// If set, the daemon (`aide.sh up`) will run this skill on schedule.
    #[serde(default)]
    pub schedule: Option<String>,
    /// Scoped environment variables for this skill.
    /// When set, ONLY these vars are injected from the vault (overrides `[env]`).
    #[serde(default)]
    pub env: Option<Vec<String>>,
    /// Human-readable description shown in `aide.sh exec <instance>` help output
    /// and in MCP tool listings.
    #[serde(default)]
    pub description: Option<String>,
    /// Usage string for help output (e.g. `"lms [courses|assignments|grades]"`).
    /// Displayed as the skill's command-line synopsis.
    #[serde(default)]
    pub usage: Option<String>,
}

/// Seed data configuration.
///
/// Points to a directory of initial knowledge files (documents, configs,
/// templates) that are bundled into the agent archive. These files are
/// copied into the instance at spawn time, providing the agent with
/// baseline context.
///
/// ```toml
/// [seed]
/// dir = "seed/"
/// ```
#[derive(Debug, Deserialize)]
pub struct SeedSection {
    /// Path to the seed directory, relative to the agent root.
    /// All files within are recursively included in the build archive.
    pub dir: String,
}

/// Environment variable declarations.
///
/// Declares which environment variables the agent needs at runtime.
/// Variables are injected from the aide vault (`aide.sh vault set KEY VALUE`).
/// Required variables cause a startup error if missing; optional variables
/// are silently omitted.
///
/// ```toml
/// [env]
/// required = ["LMS_TOKEN"]
/// optional = ["SMTP_USER", "SMTP_PASS"]
/// ```
///
/// Per-skill `env` fields can further scope which variables a specific
/// skill receives (see [`SkillDef::env`]).
#[derive(Debug, Deserialize)]
pub struct EnvSection {
    /// Variables that MUST be present in the vault. The agent will fail to
    /// start if any required variable is missing.
    #[serde(default)]
    pub required: Vec<String>,
    /// Variables that MAY be present. The agent runs without them, but
    /// features depending on these vars will be degraded.
    #[serde(default)]
    pub optional: Vec<String>,
}

impl AgentfileSpec {
    /// Load and parse `Agentfile.toml` from the given directory.
    ///
    /// Reads `<dir>/Agentfile.toml`, deserializes it into an [`AgentfileSpec`],
    /// and returns the result. This does NOT validate file references — call
    /// [`validate()`](Self::validate) separately for that.
    ///
    /// # Errors
    ///
    /// - Returns an error if `Agentfile.toml` does not exist in `dir`.
    /// - Returns an error if the file cannot be read (permissions, I/O).
    /// - Returns an error if the TOML is malformed or missing required fields
    ///   (`agent.name`, `agent.version`).
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

    /// Validate that all referenced files exist relative to the given directory.
    ///
    /// Performs the following checks:
    /// 1. **Persona file** — if `[persona]` is set, the referenced file must exist.
    /// 2. **Skill files** — each skill must have exactly one of `script` or `prompt`,
    ///    and the referenced file must exist on disk.
    /// 3. **Seed directory** — if `[seed]` is set, the directory should exist
    ///    (missing seed dir produces a warning, not an error).
    ///
    /// # Returns
    ///
    /// A `Vec<String>` of non-fatal warnings (e.g., seed dir missing, both script
    /// and prompt specified). Fatal problems cause an `Err` return.
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

    /// Generate `--help` output for `aide.sh exec <instance> --help`.
    ///
    /// Produces a human-readable help string with the following sections:
    /// - **Header**: instance name, agent name:version, and description.
    /// - **Skills**: sorted alphabetically, each showing usage, description, and env vars.
    /// - **Semantic mode hint**: shows the `-p` flag syntax for natural language queries.
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

    /// Returns the archive filename: `<name>-<version>.tar.gz`.
    ///
    /// This is the naming convention used by `aide.sh build` and the registry.
    /// For example, agent `"school-assistant"` version `"0.1.0"` produces
    /// `"school-assistant-0.1.0.tar.gz"`.
    pub fn archive_name(&self) -> String {
        format!("{}-{}.tar.gz", self.agent.name, self.agent.version)
    }

    /// Collect all files that should be included in the build archive.
    ///
    /// Returns a list of absolute paths to bundle into the `.tar.gz`. Includes:
    /// - `Agentfile.toml` (always)
    /// - The persona file (if `[persona]` is set)
    /// - All skill script and prompt files
    /// - All files in the seed directory (recursively, if `[seed]` is set)
    ///
    /// Does NOT include dotfiles, build artifacts, or anything outside the
    /// declared manifest. Call [`validate()`](Self::validate) first to ensure
    /// all referenced files actually exist.
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
