// Vault v2: multi-recipient age encryption + git-based sync
//
// Storage:
//   vault.age      — encrypted, in aide-vault git repo (private)
//   vault.key      — per-machine private key, ~/.aide/vault.key, never in git
//   recipients.txt — all machines' public keys, in vault repo
//
// Sync:
//   Each machine edits on its own branch, merges to main.
//   `aide vault sync` = git commit + push own branch, then merge to main.
//   Remote: `git pull main` to get latest.

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use tokio::process::Command;
use tracing::{info, warn};

pub struct Vault {
    /// Path to the vault repo directory (e.g., ~/claude_projects/aide-vault)
    repo_dir: PathBuf,
    /// Path to the age identity (private key) file (~/.aide/vault.key)
    identity_path: PathBuf,
}

impl Vault {
    pub fn new(repo_dir: PathBuf, identity_path: PathBuf) -> Self {
        Self {
            repo_dir,
            identity_path,
        }
    }

    /// Construct from config paths with tilde expansion
    pub fn from_config(repo_dir: &str, identity_path: Option<&str>) -> Self {
        let repo = PathBuf::from(shellexpand::tilde(repo_dir).to_string());
        let key = identity_path
            .map(|p| PathBuf::from(shellexpand::tilde(p).to_string()))
            .unwrap_or_else(|| {
                let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
                PathBuf::from(home).join(".aide").join("vault.key")
            });
        Self::new(repo, key)
    }

    fn vault_age_path(&self) -> PathBuf {
        self.repo_dir.join("vault.age")
    }

    fn recipients_path(&self) -> PathBuf {
        self.repo_dir.join("recipients.txt")
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

        // Ensure parent dir exists
        if let Some(parent) = self.identity_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
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
    pub async fn recipient(&self) -> Result<String> {
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

    /// Encrypt plaintext data into vault.age using recipients.txt
    pub async fn encrypt(&self, plaintext: &[u8]) -> Result<()> {
        let recipients = self.recipients_path();
        if !recipients.exists() {
            // Fallback: single-recipient with own key
            warn!("recipients.txt not found, using single-recipient mode");
            return self.encrypt_single(plaintext).await;
        }

        let mut child = Command::new("age")
            .arg("-R")
            .arg(&recipients)
            .arg("-o")
            .arg(&self.vault_age_path())
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

        info!(path = %self.vault_age_path().display(), "vault encrypted (multi-recipient)");
        Ok(())
    }

    /// Fallback: encrypt with single recipient (own key)
    async fn encrypt_single(&self, plaintext: &[u8]) -> Result<()> {
        let recipient = self.recipient().await?;

        let mut child = Command::new("age")
            .arg("-r")
            .arg(&recipient)
            .arg("-o")
            .arg(&self.vault_age_path())
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

        Ok(())
    }

    /// Decrypt the vault file into plaintext
    pub async fn decrypt(&self) -> Result<Vec<u8>> {
        let vault_path = self.vault_age_path();
        if !vault_path.exists() {
            bail!("vault file not found: {}", vault_path.display());
        }

        let output = Command::new("age")
            .arg("-d")
            .arg("-i")
            .arg(&self.identity_path)
            .arg(&vault_path)
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

    /// Git commit + push on current branch
    pub async fn git_commit_push(&self, message: &str) -> Result<()> {
        let repo = &self.repo_dir;

        // git add vault.age
        let status = Command::new("git")
            .args(["-C", repo.to_str().unwrap_or("."), "add", "vault.age"])
            .status()
            .await?;
        if !status.success() {
            bail!("git add failed");
        }

        // Check if there are changes to commit
        let diff = Command::new("git")
            .args(["-C", repo.to_str().unwrap_or("."), "diff", "--cached", "--quiet"])
            .status()
            .await?;

        if diff.success() {
            info!("no changes to commit");
            return Ok(());
        }

        // git commit
        let status = Command::new("git")
            .args(["-C", repo.to_str().unwrap_or("."), "commit", "-m", message])
            .status()
            .await?;
        if !status.success() {
            bail!("git commit failed");
        }

        // git push
        let status = Command::new("git")
            .args(["-C", repo.to_str().unwrap_or("."), "push"])
            .status()
            .await?;
        if !status.success() {
            warn!("git push failed — commit is local only");
        }

        Ok(())
    }

    /// Sync: pull main, merge own branch changes
    pub async fn sync(&self) -> Result<()> {
        let repo = self.repo_dir.to_str().unwrap_or(".");

        // Fetch
        let status = Command::new("git")
            .args(["-C", repo, "fetch", "origin"])
            .status()
            .await?;
        if !status.success() {
            bail!("git fetch failed");
        }

        // Pull main
        let status = Command::new("git")
            .args(["-C", repo, "checkout", "main"])
            .status()
            .await?;
        if !status.success() {
            bail!("git checkout main failed");
        }

        let status = Command::new("git")
            .args(["-C", repo, "pull", "origin", "main"])
            .status()
            .await?;
        if !status.success() {
            warn!("git pull main had issues — check for conflicts");
        }

        info!("vault synced from main");
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
            vault = %self.vault_age_path().display(),
            "imported env into vault"
        );
        Ok(())
    }

    /// Export vault back to plaintext (for reading secrets)
    pub async fn get_env(&self) -> Result<std::collections::HashMap<String, String>> {
        let data = self.decrypt().await?;
        let text = String::from_utf8(data)?;
        let mut env = std::collections::HashMap::new();

        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let line = line.strip_prefix("export ").unwrap_or(line);
            if let Some((key, value)) = line.split_once('=') {
                let value = value.trim_matches('\'').trim_matches('"');
                env.insert(key.to_string(), value.to_string());
            }
        }

        Ok(env)
    }

    /// List all key names in the vault (no values)
    pub async fn list_keys(&self) -> Result<Vec<String>> {
        let data = self.decrypt().await?;
        let text = String::from_utf8(data)?;
        let mut keys = Vec::new();

        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let line = line.strip_prefix("export ").unwrap_or(line);
            if let Some((key, _)) = line.split_once('=') {
                keys.push(key.to_string());
            }
        }

        keys.sort();
        Ok(keys)
    }
}
