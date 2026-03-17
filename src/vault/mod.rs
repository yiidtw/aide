// Vault: encrypted credential storage + cross-machine sync
// MVP: age encryption via CLI + scp push on change

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use tokio::process::Command;
use tracing::{info, warn};

/// Vault manages encrypted credentials via age CLI.
/// Plaintext env vars → age-encrypted vault file → scp to targets.
pub struct Vault {
    /// Path to the encrypted vault file (e.g., ~/.aide/vault.age)
    vault_path: PathBuf,
    /// Path to the age identity (private key) file
    identity_path: PathBuf,
    /// Sync targets (machine hostnames from aide.toml)
    targets: Vec<String>,
}

impl Vault {
    pub fn new(vault_path: PathBuf, targets: Vec<String>) -> Self {
        let identity_path = vault_path
            .parent()
            .unwrap_or(Path::new("~/.aide"))
            .join("vault.key");
        Self {
            vault_path,
            identity_path,
            targets,
        }
    }

    /// Get the identity (private key) file path
    pub fn identity_path(&self) -> PathBuf {
        self.identity_path.clone()
    }

    /// Initialize vault: generate age keypair if missing
    pub async fn init(&self) -> Result<()> {
        if self.identity_path.exists() {
            info!(path = %self.identity_path.display(), "vault identity exists");
            return Ok(());
        }

        info!("generating new age identity for vault");
        let output = Command::new("age-keygen")
            .arg("-o")
            .arg(&self.identity_path)
            .output()
            .await
            .context("failed to run age-keygen")?;

        if !output.status.success() {
            bail!(
                "age-keygen failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        info!(path = %self.identity_path.display(), "vault identity created");
        Ok(())
    }

    /// Get the public key (recipient) from the identity file
    async fn recipient(&self) -> Result<String> {
        let output = Command::new("age-keygen")
            .arg("-y")
            .arg(&self.identity_path)
            .output()
            .await
            .context("failed to extract public key")?;

        if !output.status.success() {
            bail!(
                "age-keygen -y failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        Ok(String::from_utf8(output.stdout)?.trim().to_string())
    }

    /// Encrypt plaintext data into the vault file
    pub async fn encrypt(&self, plaintext: &[u8]) -> Result<()> {
        let recipient = self.recipient().await?;

        let mut child = Command::new("age")
            .arg("-r")
            .arg(&recipient)
            .arg("-o")
            .arg(&self.vault_path)
            .stdin(std::process::Stdio::piped())
            .spawn()
            .context("failed to spawn age")?;

        if let Some(mut stdin) = child.stdin.take() {
            use tokio::io::AsyncWriteExt;
            stdin.write_all(plaintext).await?;
        }

        let status = child.wait().await?;
        if !status.success() {
            bail!("age encrypt failed");
        }

        info!(path = %self.vault_path.display(), "vault encrypted");
        Ok(())
    }

    /// Decrypt the vault file into plaintext
    pub async fn decrypt(&self) -> Result<Vec<u8>> {
        if !self.vault_path.exists() {
            bail!("vault file not found: {}", self.vault_path.display());
        }

        let output = Command::new("age")
            .arg("-d")
            .arg("-i")
            .arg(&self.identity_path)
            .arg(&self.vault_path)
            .output()
            .await
            .context("failed to run age -d")?;

        if !output.status.success() {
            bail!(
                "age decrypt failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        Ok(output.stdout)
    }

    /// Sync vault file + identity to all targets via scp
    pub async fn sync_to_targets(&self) -> Result<()> {
        if !self.vault_path.exists() {
            bail!("vault file not found, encrypt first");
        }

        for target in &self.targets {
            info!(target = %target, "syncing vault");

            // Use ~/.aide on remote (tilde-relative, not absolute)
            let status = Command::new("ssh")
                .args([target.as_str(), "mkdir -p ~/.aide"])
                .status()
                .await?;

            if !status.success() {
                warn!(target = %target, "failed to create remote dir");
                continue;
            }

            // scp vault file to remote ~/.aide/
            let vault_name = self.vault_path.file_name().unwrap_or_default().to_str().unwrap_or("vault.age");
            let remote = format!("{}:~/.aide/{}", target, vault_name);
            let status = Command::new("scp")
                .args([
                    self.vault_path.to_str().unwrap_or(""),
                    &remote,
                ])
                .status()
                .await?;

            if !status.success() {
                warn!(target = %target, "failed to sync vault file");
                continue;
            }

            // scp identity file to remote ~/.aide/
            let key_name = self.identity_path.file_name().unwrap_or_default().to_str().unwrap_or("vault.key");
            let remote_key = format!("{}:~/.aide/{}", target, key_name);
            let status = Command::new("scp")
                .args([
                    self.identity_path.to_str().unwrap_or(""),
                    &remote_key,
                ])
                .status()
                .await?;

            if status.success() {
                info!(target = %target, "vault synced");
            } else {
                warn!(target = %target, "failed to sync vault identity");
            }
        }

        Ok(())
    }

    /// Import from a plaintext env.sh file (key=value lines)
    pub async fn import_env(&self, env_path: &Path) -> Result<()> {
        let content = tokio::fs::read(env_path)
            .await
            .with_context(|| format!("failed to read {}", env_path.display()))?;

        self.init().await?;
        self.encrypt(&content).await?;

        info!(
            source = %env_path.display(),
            vault = %self.vault_path.display(),
            "imported env into vault"
        );
        Ok(())
    }

    /// Export vault back to plaintext (for reading secrets)
    #[allow(dead_code)]
    pub async fn get_env(&self) -> Result<std::collections::HashMap<String, String>> {
        let data = self.decrypt().await?;
        let text = String::from_utf8(data)?;
        let mut env = std::collections::HashMap::new();

        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            // Handle export KEY=VALUE or KEY=VALUE
            let line = line.strip_prefix("export ").unwrap_or(line);
            if let Some((key, value)) = line.split_once('=') {
                let value = value.trim_matches('\'').trim_matches('"');
                env.insert(key.to_string(), value.to_string());
            }
        }

        Ok(env)
    }
}
