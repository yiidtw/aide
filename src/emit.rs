//! Generate .claude/agents/*.md wrappers for registered aide agents.

use anyhow::Result;
use std::path::PathBuf;

use crate::{aidefile, registry};

pub fn emit_claude_agents(output_dir: &str) -> Result<()> {
    let agents = registry::list()?;
    if agents.is_empty() {
        println!("No agents registered. Nothing to emit.");
        return Ok(());
    }

    let out = PathBuf::from(output_dir);
    std::fs::create_dir_all(&out)?;

    let mut count = 0u32;
    for agent in &agents {
        let path = PathBuf::from(shellexpand::tilde(&agent.path).as_ref());

        // Load Aidefile to get metadata; skip agents with broken/missing Aidefiles
        let af = match aidefile::load(&path) {
            Ok(af) => af,
            Err(e) => {
                eprintln!(
                    "warning: skipping '{}' — cannot load Aidefile: {e}",
                    agent.name
                );
                continue;
            }
        };

        let content = format!(
            r#"---
name: {name}
description: "Dispatch {name} work via aide. Runs in isolated token budget."
tools:
  - Bash
---

You are a thin dispatch wrapper for the `{name}` aide agent.

## Rules
- You ONLY run `aide dispatch` and `aide wait` commands
- Do NOT attempt to do the work yourself
- Do NOT read or edit files in other repositories

## Workflow
1. Run: `aide dispatch {name} "{{task description from user}}"`
2. Capture the issue reference from output (e.g. owner/repo#N)
3. Run: `aide wait {{issue_ref}}`
4. Return the summary to the user

## Agent info
- **Trigger**: {trigger}
- **Budget**: {budget}
- **Path**: {path}
"#,
            name = agent.name,
            trigger = af.trigger.on,
            budget = af.budget.tokens,
            path = agent.path,
        );

        let file_path = out.join(format!("{}.md", agent.name));
        std::fs::write(&file_path, content)?;
        count += 1;
    }

    println!("Generated {count} agent wrapper(s) in {output_dir}");
    Ok(())
}
