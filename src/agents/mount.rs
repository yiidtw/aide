use anyhow::{bail, Result};
use std::fs;
use std::path::{Path, PathBuf};

/// Gather the combined content from an instance directory for mounting.
/// Reads persona.md, knowledge/*.md, memory/*.md, and instance.toml metadata.
fn gather_instance_content(instance_dir: &Path, instance_name: &str) -> Result<String> {
    let mut sections = Vec::new();

    // Header
    sections.push(format!(
        "# Agent: {}\n\nThis context was mounted by `aide mount`. Do not edit manually.\n",
        instance_name
    ));

    // Instance metadata from instance.toml (try cognition/ first, fall back to root)
    let manifest_path = super::instance::resolve_path(instance_dir, "cognition/instance.toml", "instance.toml");
    if manifest_path.exists() {
        let raw = fs::read_to_string(&manifest_path)?;
        if let Ok(manifest) = toml::from_str::<toml::Value>(&raw) {
            let mut meta_lines = vec!["## Instance Metadata\n".to_string()];
            if let Some(t) = manifest.get("agent_type").and_then(|v| v.as_str()) {
                meta_lines.push(format!("- **Type:** {}", t));
            }
            if let Some(e) = manifest.get("email").and_then(|v| v.as_str()) {
                meta_lines.push(format!("- **Email:** {}", e));
            }
            if let Some(r) = manifest.get("role").and_then(|v| v.as_str()) {
                meta_lines.push(format!("- **Role:** {}", r));
            }
            // Cron schedules
            if let Some(cron) = manifest.get("cron").and_then(|v| v.as_array()) {
                if !cron.is_empty() {
                    meta_lines.push("\n### Cron Schedules\n".to_string());
                    for entry in cron {
                        let schedule = entry.get("schedule").and_then(|v| v.as_str()).unwrap_or("?");
                        let skill = entry.get("skill").and_then(|v| v.as_str()).unwrap_or("?");
                        meta_lines.push(format!("- `{}` — {}", schedule, skill));
                    }
                }
            }
            sections.push(meta_lines.join("\n"));
        }
    }

    // Persona (try occupation/persona.md first, fall back to root)
    let persona_path = super::instance::resolve_path(instance_dir, "occupation/persona.md", "persona.md");
    if persona_path.exists() {
        let content = fs::read_to_string(&persona_path)?;
        if !content.trim().is_empty() {
            sections.push(format!("## Persona\n\n{}", content.trim()));
        }
    }

    // Knowledge files (check occupation/knowledge/ first, then knowledge/, fall back to legacy seed/)
    let occ_knowledge_dir = instance_dir.join("occupation/knowledge");
    let knowledge_dir = instance_dir.join("knowledge");
    let legacy_seed_dir = instance_dir.join("seed");
    let kb_dir = if occ_knowledge_dir.is_dir() { &occ_knowledge_dir }
        else if knowledge_dir.is_dir() { &knowledge_dir }
        else { &legacy_seed_dir };
    if kb_dir.is_dir() {
        let mut kb_files = collect_md_files(kb_dir)?;
        kb_files.sort();
        if !kb_files.is_empty() {
            let mut kb_section = vec!["## Knowledge\n".to_string()];
            for path in &kb_files {
                let rel = path.strip_prefix(kb_dir).unwrap_or(path);
                let content = fs::read_to_string(path)?;
                if !content.trim().is_empty() {
                    kb_section.push(format!("### {}\n\n{}", rel.display(), content.trim()));
                }
            }
            sections.push(kb_section.join("\n\n"));
        }
    }

    // Memory files (try cognition/memory/ first, fall back to memory/)
    let memory_dir = super::instance::resolve_path(instance_dir, "cognition/memory", "memory");
    if memory_dir.is_dir() {
        let mut mem_files = collect_md_files(&memory_dir)?;
        mem_files.sort();
        if !mem_files.is_empty() {
            let mut mem_section = vec!["## Memory\n".to_string()];
            for path in &mem_files {
                let rel = path.strip_prefix(&memory_dir).unwrap_or(path);
                let content = fs::read_to_string(path)?;
                if !content.trim().is_empty() {
                    mem_section.push(format!("### {}\n\n{}", rel.display(), content.trim()));
                }
            }
            sections.push(mem_section.join("\n\n"));
        }
    }

    Ok(sections.join("\n\n---\n\n"))
}

/// Collect all .md files recursively under a directory.
fn collect_md_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut results = Vec::new();
    if !dir.is_dir() {
        return Ok(results);
    }
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            results.extend(collect_md_files(&path)?);
        } else if path.extension().map(|e| e == "md").unwrap_or(false) {
            results.push(path);
        }
    }
    Ok(results)
}

// ─── Claude ───

/// Encode a cwd path into the Claude projects key format.
/// `/home/user/projects/myapp` -> `-home-user-projects-myapp`
fn claude_path_key(cwd: &Path) -> String {
    let abs = cwd.to_string_lossy();
    abs.replace('/', "-")
}

fn claude_memory_dir(cwd: &Path) -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let key = claude_path_key(cwd);
    PathBuf::from(home)
        .join(".claude")
        .join("projects")
        .join(key)
        .join("memory")
}

const MOUNT_MARKER: &str = "<!-- aide-mount -->";

pub fn mount_claude(instance_dir: &Path, instance_name: &str) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let memory_dir = claude_memory_dir(&cwd);
    fs::create_dir_all(&memory_dir)?;

    let content = gather_instance_content(instance_dir, instance_name)?;

    // Write the agent file
    let agent_file = memory_dir.join(format!("aide_{}.md", instance_name));
    let marked_content = format!("{}\n{}", MOUNT_MARKER, content);
    fs::write(&agent_file, &marked_content)?;

    // Update MEMORY.md index if it exists
    let memory_index = memory_dir.join("MEMORY.md");
    if memory_index.exists() {
        let existing = fs::read_to_string(&memory_index)?;
        let entry_line = format!(
            "- [aide_{}.md](aide_{}.md) — aide agent {} context",
            instance_name, instance_name, instance_name
        );
        if !existing.contains(&entry_line) {
            // Append under an aide section
            let aide_header = "## Aide Agents";
            if existing.contains(aide_header) {
                // Add entry after the header
                let updated = existing.replacen(
                    aide_header,
                    &format!("{}\n{}", aide_header, entry_line),
                    1,
                );
                fs::write(&memory_index, updated)?;
            } else {
                // Append a new section
                let mut updated = existing.clone();
                if !updated.ends_with('\n') {
                    updated.push('\n');
                }
                updated.push_str(&format!("\n{}\n{}\n", aide_header, entry_line));
                fs::write(&memory_index, updated)?;
            }
        }
    }

    println!("mounted {} -> claude ({})", instance_name, agent_file.display());
    Ok(())
}

pub fn unmount_claude(instance_name: &str) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let memory_dir = claude_memory_dir(&cwd);

    let agent_file = memory_dir.join(format!("aide_{}.md", instance_name));
    if agent_file.exists() {
        fs::remove_file(&agent_file)?;
        println!("unmounted {} from claude", instance_name);
    } else {
        println!("no claude mount found for {}", instance_name);
    }

    // Clean up MEMORY.md index
    let memory_index = memory_dir.join("MEMORY.md");
    if memory_index.exists() {
        let existing = fs::read_to_string(&memory_index)?;
        let entry_line = format!(
            "- [aide_{}.md](aide_{}.md) — aide agent {} context",
            instance_name, instance_name, instance_name
        );
        if existing.contains(&entry_line) {
            let updated = existing.replace(&format!("{}\n", entry_line), "");
            // Also remove empty Aide Agents section header if nothing left
            let updated = updated.replace("\n## Aide Agents\n\n", "\n");
            fs::write(&memory_index, updated)?;
        }
    }

    Ok(())
}

// ─── Codex ───

pub fn mount_codex(instance_dir: &Path, instance_name: &str) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let agents_file = cwd.join("AGENTS.md");

    let content = gather_instance_content(instance_dir, instance_name)?;
    let marked_content = format!("{}\n{}", MOUNT_MARKER, content);

    // If AGENTS.md exists and has non-aide content, preserve it
    if agents_file.exists() {
        let existing = fs::read_to_string(&agents_file)?;
        if !existing.contains(MOUNT_MARKER) {
            // Append aide content after existing content
            let combined = format!(
                "{}\n\n---\n\n{}\n{}",
                existing.trim(),
                MOUNT_MARKER,
                content
            );
            fs::write(&agents_file, combined)?;
            println!("mounted {} -> codex (appended to {})", instance_name, agents_file.display());
            return Ok(());
        }
    }

    fs::write(&agents_file, &marked_content)?;
    println!("mounted {} -> codex ({})", instance_name, agents_file.display());
    Ok(())
}

pub fn unmount_codex(instance_name: &str) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let agents_file = cwd.join("AGENTS.md");

    if !agents_file.exists() {
        println!("no codex mount found for {}", instance_name);
        return Ok(());
    }

    let existing = fs::read_to_string(&agents_file)?;
    if let Some(marker_pos) = existing.find(MOUNT_MARKER) {
        let before = existing[..marker_pos].trim_end();
        if before.is_empty() {
            fs::remove_file(&agents_file)?;
        } else {
            fs::write(&agents_file, format!("{}\n", before))?;
        }
        println!("unmounted {} from codex", instance_name);
    } else {
        println!("no aide mount marker found in AGENTS.md for {}", instance_name);
    }

    Ok(())
}

// ─── Gemini ───

pub fn mount_gemini(instance_dir: &Path, instance_name: &str) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let gemini_file = cwd.join("GEMINI.md");

    let content = gather_instance_content(instance_dir, instance_name)?;
    let marked_content = format!("{}\n{}", MOUNT_MARKER, content);

    // If GEMINI.md exists and has non-aide content, preserve it
    if gemini_file.exists() {
        let existing = fs::read_to_string(&gemini_file)?;
        if !existing.contains(MOUNT_MARKER) {
            let combined = format!(
                "{}\n\n---\n\n{}\n{}",
                existing.trim(),
                MOUNT_MARKER,
                content
            );
            fs::write(&gemini_file, combined)?;
            println!("mounted {} -> gemini (appended to {})", instance_name, gemini_file.display());
            return Ok(());
        }
    }

    fs::write(&gemini_file, &marked_content)?;
    println!("mounted {} -> gemini ({})", instance_name, gemini_file.display());
    Ok(())
}

pub fn unmount_gemini(instance_name: &str) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let gemini_file = cwd.join("GEMINI.md");

    if !gemini_file.exists() {
        println!("no gemini mount found for {}", instance_name);
        return Ok(());
    }

    let existing = fs::read_to_string(&gemini_file)?;
    if let Some(marker_pos) = existing.find(MOUNT_MARKER) {
        let before = existing[..marker_pos].trim_end();
        if before.is_empty() {
            fs::remove_file(&gemini_file)?;
        } else {
            fs::write(&gemini_file, format!("{}\n", before))?;
        }
        println!("unmounted {} from gemini", instance_name);
    } else {
        println!("no aide mount marker found in GEMINI.md for {}", instance_name);
    }

    Ok(())
}

// ─── Dispatch ───

pub fn mount(instance_dir: &Path, instance_name: &str, target: &str) -> Result<()> {
    match target {
        "claude" => mount_claude(instance_dir, instance_name),
        "codex" => mount_codex(instance_dir, instance_name),
        "gemini" => mount_gemini(instance_dir, instance_name),
        "all" => {
            mount_claude(instance_dir, instance_name)?;
            mount_codex(instance_dir, instance_name)?;
            mount_gemini(instance_dir, instance_name)?;
            Ok(())
        }
        _ => bail!(
            "unknown mount target '{}'. Valid targets: claude, codex, gemini, all",
            target
        ),
    }
}

pub fn unmount(instance_name: &str, target: &str) -> Result<()> {
    match target {
        "claude" => unmount_claude(instance_name),
        "codex" => unmount_codex(instance_name),
        "gemini" => unmount_gemini(instance_name),
        "all" => {
            unmount_claude(instance_name)?;
            unmount_codex(instance_name)?;
            unmount_gemini(instance_name)?;
            Ok(())
        }
        _ => bail!(
            "unknown unmount target '{}'. Valid targets: claude, codex, gemini, all",
            target
        ),
    }
}
