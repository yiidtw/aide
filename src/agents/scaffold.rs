use anyhow::{bail, Result};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

/// Generate a new agent project skeleton.
pub fn init_agent(name: &str, dir: &Path) -> Result<()> {
    if dir.exists() {
        bail!("directory already exists: {}", dir.display());
    }

    // Create directory structure
    fs::create_dir_all(dir.join("skills"))?;
    fs::create_dir_all(dir.join("seed"))?;

    // Agentfile.toml
    let agentfile = format!(
        r#"[agent]
name = "{name}"
version = "0.1.0"
description = "TODO: describe your agent"
author = "TODO: your username"

[persona]
file = "persona.md"

[skills.hello]
script = "skills/hello.sh"
description = "A greeting skill"
usage = "hello [name]"
# schedule = "0 9 * * *"    # uncomment for cron
# env = ["MY_API_KEY"]      # uncomment for secrets

[seed]
dir = "seed/"

[env]
required = []
optional = []
"#
    );
    fs::write(dir.join("Agentfile.toml"), agentfile)?;

    // persona.md
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
    fs::write(dir.join("persona.md"), persona)?;

    // skills/hello.sh
    let hello = format!(
        r#"#!/usr/bin/env bash
# hello — a greeting skill
# usage: hello [name]

NAME="${{1:-world}}"
echo "Hello, ${{NAME}}! I am {name}."
"#
    );
    let hello_path = dir.join("skills/hello.sh");
    fs::write(&hello_path, hello)?;
    fs::set_permissions(&hello_path, fs::Permissions::from_mode(0o755))?;

    // seed/.gitkeep
    fs::write(dir.join("seed/.gitkeep"), "")?;

    Ok(())
}
