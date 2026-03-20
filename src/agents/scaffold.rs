use anyhow::{bail, Result};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

/// Generate a new agent project skeleton.
///
/// Creates a complete, lint-passing agent directory at `dir` with the
/// following structure:
///
/// ```text
/// <name>/
///   occupation/          # shareable job definition
///     Agentfile.toml     # manifest with [agent], [persona], [skills.hello], [knowledge], [env]
///     persona.md         # starter persona with TODOs
///     skills/
///       hello.ts         # example TypeScript skill (runs via bun)
///     knowledge/
///       .gitkeep         # placeholder so the directory is tracked by git
///   cognition/           # instance-specific brain
///     memory/
///       .gitkeep
///     logs/              # empty dir
///   .aideignore          # cognition/
///   README.md
/// ```
///
/// The generated `Agentfile.toml` includes commented-out examples of
/// `schedule` and `env` fields for quick reference.
///
/// # Errors
///
/// Returns an error if the target directory already exists (to prevent
/// accidental overwrites). Use a fresh name or remove the existing directory.
pub fn init_agent(name: &str, dir: &Path) -> Result<()> {
    if dir.exists() {
        bail!("directory already exists: {}", dir.display());
    }

    // Create directory structure
    fs::create_dir_all(dir.join("occupation/skills"))?;
    fs::create_dir_all(dir.join("occupation/knowledge"))?;
    fs::create_dir_all(dir.join("cognition/memory"))?;
    fs::create_dir_all(dir.join("cognition/logs"))?;

    // occupation/Agentfile.toml
    let agentfile = format!(
        r#"[agent]
name = "{name}"
version = "0.1.0"
description = "TODO: describe your agent"
author = "TODO: your username"

[persona]
file = "persona.md"

[skills.hello]
script = "skills/hello.ts"
description = "A greeting skill"
usage = "hello [name]"
# schedule = "0 9 * * *"    # uncomment for cron
# env = ["MY_API_KEY"]      # uncomment for secrets

[knowledge]
dir = "knowledge/"

[env]
required = []
optional = []

[limits]
max_timeout = 300
max_tokens = 4096
max_retry = 3
"#
    );
    fs::write(dir.join("occupation/Agentfile.toml"), agentfile)?;

    // occupation/persona.md
    let persona = format!(
        r#"# {name}

You are {name}, an AI agent.

## Role
TODO: describe what this agent does

## Behavior
- Be helpful and concise
- TODO: add behavioral guidelines
"#
    );
    fs::write(dir.join("occupation/persona.md"), persona)?;

    // occupation/skills/hello.ts
    let hello = format!(
        r#"// hello — a greeting skill
// usage: hello [name]
const name = process.argv[2] || "world";
console.log(`Hello, ${{name}}! I'm your aide agent.`);
"#
    );
    let hello_path = dir.join("occupation/skills/hello.ts");
    fs::write(&hello_path, hello)?;
    fs::set_permissions(&hello_path, fs::Permissions::from_mode(0o755))?;

    // occupation/knowledge/.gitkeep
    fs::write(dir.join("occupation/knowledge/.gitkeep"), "")?;

    // cognition/memory/.gitkeep
    fs::write(dir.join("cognition/memory/.gitkeep"), "")?;

    // .aideignore
    fs::write(dir.join(".aideignore"), "cognition/\n")?;

    // README.md
    let readme = format!(
        r#"# {name}

An aide.sh agent.

## Structure

- `occupation/` — shareable job definition (Agentfile, persona, skills, knowledge)
- `cognition/` — instance-specific brain (memory, logs)

Powered by [aide.sh](https://aide.sh) — Deploy AI agents, just like Docker.
"#
    );
    fs::write(dir.join("README.md"), readme)?;

    Ok(())
}
