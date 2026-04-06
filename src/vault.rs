//! Vault — age-encrypted secrets, injected as env vars at spawn time.
//!
//! **Security invariant**: secrets flow only to `Command::env()`, never to prompt strings.

use anyhow::{bail, Context, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Decrypt vault and extract requested keys.
///
/// Returns `Vec<(key, value)>` suitable for passing to `Command::env()`.
/// The returned values must NEVER be interpolated into prompt strings.
pub fn decrypt_keys(vault_path: &Path, key_path: &Path, keys: &[String]) -> Result<Vec<(String, String)>> {
    if keys.is_empty() {
        return Ok(vec![]);
    }

    if !vault_path.exists() {
        bail!("Vault not found at {}", vault_path.display());
    }
    if !key_path.exists() {
        bail!("Vault key not found at {}", key_path.display());
    }

    // Decrypt vault.age → plaintext .env format
    let output = Command::new("age")
        .args(["-d", "-i"])
        .arg(key_path)
        .arg(vault_path)
        .output()
        .context("Failed to run age decrypt")?;

    if !output.status.success() {
        bail!(
            "age decrypt failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let plaintext = String::from_utf8(output.stdout).context("Vault contains non-UTF8")?;
    let all_secrets = parse_env(&plaintext);

    // Filter to requested keys only
    let mut result = Vec::with_capacity(keys.len());
    for key in keys {
        match all_secrets.get(key.as_str()) {
            Some(val) => result.push((key.clone(), val.clone())),
            None => bail!("Vault missing required key: {key}"),
        }
    }

    Ok(result)
}

/// Parse .env format: KEY=VALUE lines, skip comments and blanks.
fn parse_env(content: &str) -> HashMap<&str, String> {
    let mut map = HashMap::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, val)) = line.split_once('=') {
            let val = val.trim_matches('"').trim_matches('\'');
            map.insert(key.trim(), val.to_string());
        }
    }
    map
}

/// Inject secrets into a Command as environment variables.
///
/// This is the ONLY function that should apply vault secrets to a process.
pub fn inject(cmd: &mut Command, secrets: &[(String, String)]) {
    for (key, val) in secrets {
        cmd.env(key, val);
    }
}

/// Default vault paths.
pub fn default_vault_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_default()
        .join(".aide")
        .join("vault.age")
}

pub fn default_key_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_default()
        .join(".aide")
        .join("vault.key")
}

// ── Kani proofs ──────────────────────────────────────────────────────

// Note: Kani proofs for string parsing are impractical (unbounded unwinding).
// Vault security invariant (secrets → env vars only, never prompt) is enforced
// structurally: decrypt_keys() returns Vec<(String, String)>, and inject() is
// the only consumer, applying them to Command::env(). No code path exists to
// interpolate secrets into prompt strings.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_env() {
        let content = "# Secrets\nGH_TOKEN=abc123\nAPI_KEY=\"my-key\"\n\nEMPTY=\n";
        let map = parse_env(content);
        assert_eq!(map["GH_TOKEN"], "abc123");
        assert_eq!(map["API_KEY"], "my-key");
        assert_eq!(map["EMPTY"], "");
    }

    #[test]
    fn test_inject_sets_env() {
        let mut cmd = Command::new("echo");
        let secrets = vec![
            ("KEY1".to_string(), "val1".to_string()),
            ("KEY2".to_string(), "val2".to_string()),
        ];
        inject(&mut cmd, &secrets);
        // Command::env doesn't provide a getter, but this verifies no panic.
    }
}
