pub mod agentfile;
pub mod commit;
pub mod instance;
pub mod lint;
pub mod mount;
pub mod registry;
pub mod scaffold;

use std::path::PathBuf;

/// Enumerate all globally available aide-skill commands.
/// Scans ~/claude_projects/aide-skill/*/skill.toml and returns a formatted prompt section.
pub fn enumerate_aide_skills() -> String {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let skill_root = PathBuf::from(&home).join("claude_projects/aide-skill");
    if !skill_root.exists() { return String::new(); }

    let entries = match std::fs::read_dir(&skill_root) {
        Ok(e) => e,
        Err(_) => return String::new(),
    };

    let mut lines = vec!["\n### Global aide-skills (use via EXEC: <skill> [args])".to_string()];
    let mut found = false;

    let mut dirs: Vec<_> = entries.filter_map(|e| e.ok()).collect();
    dirs.sort_by_key(|e| e.file_name());

    for entry in dirs {
        let skill_toml = entry.path().join("skill.toml");
        if !skill_toml.exists() { continue; }
        let content = match std::fs::read_to_string(&skill_toml) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let name = content.lines()
            .find(|l| l.trim_start().starts_with("name ="))
            .and_then(|l| l.split_once('='))
            .map(|(_, v)| v.trim().trim_matches('"').to_string())
            .unwrap_or_else(|| entry.file_name().to_string_lossy().to_string());
        let description = content.lines()
            .find(|l| l.trim_start().starts_with("description ="))
            .and_then(|l| l.split_once('='))
            .map(|(_, v)| v.trim().trim_matches('"').to_string())
            .unwrap_or_default();
        let commands = content.lines()
            .find(|l| l.trim_start().starts_with("commands ="))
            .and_then(|l| l.split_once('='))
            .map(|(_, v)| v.trim().trim_matches(|c| c == '[' || c == ']')
                .split(',')
                .map(|s| s.trim().trim_matches('"').to_string())
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(", "))
            .unwrap_or_default();

        let cmd_hint = if commands.is_empty() { String::new() } else { format!(" [{}]", commands) };
        lines.push(format!("- **{}**{} — {}", name, cmd_hint, description));
        found = true;
    }

    if found { lines.join("\n") } else { String::new() }
}
