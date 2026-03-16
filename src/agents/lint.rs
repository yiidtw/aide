use anyhow::Result;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use super::agentfile::AgentfileSpec;

const CREDENTIAL_PREFIXES: &[&str] = &[
    "sk-ant-",
    "sk-proj-",
    "AKIA",
    "ghp_",
    "gho_",
    "eyJhbG",
    "-----BEGIN",
];

pub struct LintResult {
    pub passed: Vec<String>,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

pub fn lint_agent(dir: &Path) -> Result<LintResult> {
    let mut passed = Vec::new();
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    // 1. Parse Agentfile.toml
    let spec = match AgentfileSpec::load(dir) {
        Ok(s) => {
            passed.push("Agentfile.toml parsed".into());
            s
        }
        Err(e) => {
            errors.push(format!("Agentfile.toml: {e}"));
            return Ok(LintResult {
                passed,
                errors,
                warnings,
            });
        }
    };

    // 2. name and version (guaranteed by serde, but confirm non-empty)
    if spec.agent.name.is_empty() {
        errors.push("agent.name is empty".into());
    } else {
        passed.push(format!("agent.name = {:?}", spec.agent.name));
    }
    if spec.agent.version.is_empty() {
        errors.push("agent.version is empty".into());
    } else {
        passed.push(format!("agent.version = {:?}", spec.agent.version));
    }

    // 3. description present and not TODO
    match &spec.agent.description {
        None => errors.push("agent.description is missing".into()),
        Some(d) if d.is_empty() || d.starts_with("TODO") => {
            errors.push(format!("agent.description is incomplete: {:?}", d));
        }
        Some(_) => passed.push("agent.description present".into()),
    }

    // 4. author present and not TODO
    match &spec.agent.author {
        None => errors.push("agent.author is missing".into()),
        Some(a) if a.is_empty() || a.starts_with("TODO") => {
            errors.push(format!("agent.author is incomplete: {:?}", a));
        }
        Some(_) => passed.push("agent.author present".into()),
    }

    // 5. Persona file exists
    if let Some(persona) = &spec.persona {
        let p = dir.join(&persona.file);
        if p.exists() {
            passed.push(format!("{} exists", persona.file));
        } else {
            errors.push(format!("{}: not found", persona.file));
        }
    }

    // Collect all env vars referenced by skills
    let mut skill_env_vars: Vec<String> = Vec::new();

    // 6-9, 12-13. Skill checks
    for (name, skill) in &spec.skills {
        // 6. Must have script or prompt, not both, not neither
        let has_script = skill.script.is_some();
        let has_prompt = skill.prompt.is_some();
        if !has_script && !has_prompt {
            errors.push(format!("skills.{name}: must have either 'script' or 'prompt'"));
        } else if has_script && has_prompt {
            errors.push(format!(
                "skills.{name}: has both 'script' and 'prompt' (pick one)"
            ));
        }

        // 7-8. Script file exists and is executable
        if let Some(script) = &skill.script {
            let script_path = dir.join(script);
            if script_path.exists() {
                let meta = fs::metadata(&script_path)?;
                let mode = meta.permissions().mode();
                if mode & 0o111 != 0 {
                    passed.push(format!("{script} exists (executable)"));
                } else {
                    errors.push(format!("{script}: not executable"));
                }
            } else {
                errors.push(format!("{script}: not found"));
            }
        }

        // 9. Prompt file exists
        if let Some(prompt) = &skill.prompt {
            let prompt_path = dir.join(prompt);
            if prompt_path.exists() {
                passed.push(format!("{prompt} exists"));
            } else {
                errors.push(format!("{prompt}: not found"));
            }
        }

        // 10. Cron schedule validation
        if let Some(schedule) = &skill.schedule {
            if let Err(e) = validate_cron(schedule) {
                errors.push(format!("skills.{name}: invalid schedule: {e}"));
            } else {
                passed.push(format!("skills.{name}: schedule is valid"));
            }
        }

        // 12. Skill missing description
        if skill.description.is_none() {
            warnings.push(format!("skills.{name}: missing description"));
        }

        // 13. Skill missing usage
        if skill.usage.is_none() {
            warnings.push(format!("skills.{name}: missing usage"));
        }

        // Collect per-skill env vars for check 15-16
        if let Some(env_vars) = &skill.env {
            skill_env_vars.extend(env_vars.iter().cloned());
        }
    }

    // 14. Seed directory exists if declared
    if let Some(seed) = &spec.seed {
        let seed_path = dir.join(&seed.dir);
        if !seed_path.exists() {
            warnings.push(format!("seed directory not found: {}", seed.dir));
        } else {
            passed.push(format!("seed directory {} exists", seed.dir));
        }
    }

    // 15. [env] section missing when skills reference env vars
    if !skill_env_vars.is_empty() && spec.env.is_none() {
        warnings.push("[env] section missing but skills reference env vars".into());
    }

    // 16. Per-skill env vars not in [env].required or [env].optional
    if !skill_env_vars.is_empty() {
        if let Some(env) = &spec.env {
            for var in &skill_env_vars {
                if !env.required.contains(var) && !env.optional.contains(var) {
                    warnings.push(format!(
                        "env var {var:?} used in skill but not in [env].required or [env].optional"
                    ));
                }
            }
        }
    }

    // 11. Credential leak scan
    scan_for_credentials(dir, dir, &mut errors)?;

    Ok(LintResult {
        passed,
        errors,
        warnings,
    })
}

/// Basic cron validation: 5 space-separated fields, each field is *, number, */N, N-M, or comma-separated.
fn validate_cron(expr: &str) -> Result<(), String> {
    let fields: Vec<&str> = expr.split_whitespace().collect();
    if fields.len() != 5 {
        return Err(format!(
            "expected 5 fields, got {}",
            fields.len()
        ));
    }
    for (i, field) in fields.iter().enumerate() {
        let parts: Vec<&str> = field.split(',').collect();
        for part in parts {
            if !is_valid_cron_part(part) {
                return Err(format!("invalid field {} value: {:?}", i + 1, part));
            }
        }
    }
    Ok(())
}

fn is_valid_cron_part(part: &str) -> bool {
    if part == "*" {
        return true;
    }
    // */N
    if let Some(n) = part.strip_prefix("*/") {
        return n.parse::<u32>().is_ok();
    }
    // N-M
    if part.contains('-') {
        let pieces: Vec<&str> = part.splitn(2, '-').collect();
        return pieces.len() == 2
            && pieces[0].parse::<u32>().is_ok()
            && pieces[1].parse::<u32>().is_ok();
    }
    // plain number
    part.parse::<u32>().is_ok()
}

/// Scan all text files in the directory tree for credential prefixes.
/// `root` is the top-level agent directory (for display), `dir` is the current directory being scanned.
fn scan_for_credentials(root: &Path, dir: &Path, errors: &mut Vec<String>) -> Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            scan_for_credentials(root, &path, errors)?;
        } else {
            // Only scan text-like files (skip binaries)
            if let Ok(content) = fs::read_to_string(&path) {
                for prefix in CREDENTIAL_PREFIXES {
                    if content.contains(prefix) {
                        let relative = path.strip_prefix(root).unwrap_or(&path);
                        errors.push(format!(
                            "{}: possible credential leak (contains {:?})",
                            relative.display(),
                            prefix
                        ));
                        break; // one error per file is enough
                    }
                }
            }
        }
    }
    Ok(())
}

pub fn print_lint_result(result: &LintResult) {
    for msg in &result.passed {
        println!("\u{2713} {msg}");
    }
    for msg in &result.warnings {
        println!("\u{26a0} {msg}");
    }
    for msg in &result.errors {
        println!("\u{2717} {msg}");
    }

    let w = result.warnings.len();
    let e = result.errors.len();
    if w == 0 && e == 0 {
        println!("All checks passed.");
    } else {
        println!("{w} warning(s), {e} error(s)");
    }
}
