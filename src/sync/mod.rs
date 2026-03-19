// Sync: vault, skills, and memory sync across machines
// MVP: age vault sync via scp, git pull for skills

use anyhow::{Context, Result};
use std::path::Path;
use tokio::process::Command;
use tracing::{info, warn};

use crate::config::AideConfig;
use crate::vault::Vault;

/// Sync vault to all configured targets
pub async fn sync_vault(config: &AideConfig) -> Result<()> {
    let vault_path = config
        .aide
        .vault_path
        .as_ref()
        .context("no vault_path in config")?;

    let vault_path = shellexpand::tilde(vault_path);
    let vault_path = Path::new(vault_path.as_ref());

    let target_names = config
        .sync
        .vault
        .as_ref()
        .map(|v| v.targets.clone())
        .unwrap_or_default();

    if target_names.is_empty() {
        warn!("no sync targets configured for vault");
        return Ok(());
    }

    // Resolve machine names to hostnames
    let targets: Vec<String> = target_names
        .iter()
        .filter_map(|name| {
            config.machines.get(name).map(|m| m.host.clone()).or_else(|| {
                warn!(name = %name, "unknown machine in vault targets");
                None
            })
        })
        .collect();

    let vault = Vault::from_config(
        &vault_path.to_string_lossy(),
        None,
    );
    vault.git_commit_push("vault sync").await?;

    info!("vault sync complete");
    Ok(())
}

/// Sync skills via git pull on targets
pub async fn sync_skills(config: &AideConfig) -> Result<()> {
    let skills_config = config
        .sync
        .skills
        .as_ref()
        .context("no skills sync config")?;

    let repo = skills_config
        .repo
        .as_ref()
        .context("no skills repo configured")?;

    info!(repo = %repo, "syncing skills");

    // For each machine that is a target, ssh and git pull
    for (name, machine) in &config.machines {
        if machine.host == "localhost" {
            // Local: git pull in skills directory
            let skills_dir = format!(
                "{}/skills",
                shellexpand::tilde(&config.aide.data_dir)
            );

            if Path::new(&skills_dir).exists() {
                let output = Command::new("git")
                    .args(["pull", "--ff-only"])
                    .current_dir(&skills_dir)
                    .output()
                    .await?;

                if output.status.success() {
                    info!(machine = %name, "skills updated locally");
                } else {
                    warn!(
                        machine = %name,
                        error = %String::from_utf8_lossy(&output.stderr),
                        "local skills pull failed"
                    );
                }
            } else {
                info!(machine = %name, dir = %skills_dir, "skills dir not found, skipping");
            }
        } else {
            // Remote: ssh + git pull
            let remote_skills = format!(
                "{}/skills",
                shellexpand::tilde(&config.aide.data_dir)
            );

            let output = Command::new("ssh")
                .args([
                    &machine.host,
                    &format!(
                        "cd {} && git pull --ff-only 2>&1 || echo 'skills dir not found'",
                        remote_skills
                    ),
                ])
                .output()
                .await?;

            if output.status.success() {
                let out = String::from_utf8_lossy(&output.stdout);
                info!(machine = %name, output = %out.trim(), "remote skills sync");
            } else {
                warn!(machine = %name, "failed to sync skills to remote");
            }
        }
    }

    Ok(())
}

/// Show sync status across machines
pub async fn sync_status(config: &AideConfig) -> Result<()> {
    println!("=== aide sync status ===\n");

    // Vault status
    if let Some(vault_path) = &config.aide.vault_path {
        let expanded = shellexpand::tilde(vault_path);
        let path = Path::new(expanded.as_ref());
        if path.exists() {
            let meta = std::fs::metadata(path)?;
            let modified = meta
                .modified()
                .ok()
                .and_then(|t| {
                    t.duration_since(std::time::UNIX_EPOCH)
                        .ok()
                        .map(|d| d.as_secs())
                })
                .unwrap_or(0);
            println!(
                "vault: {} (modified: {})",
                expanded,
                modified
            );
        } else {
            println!("vault: {} (not found)", expanded);
        }
    }

    // Check remote vault timestamps
    if let Some(sync_vault) = &config.sync.vault {
        for target in &sync_vault.targets {
            let vault_path = config
                .aide
                .vault_path
                .as_deref()
                .unwrap_or("~/.aide/vault.age");
            let expanded = shellexpand::tilde(vault_path);

            let output = Command::new("ssh")
                .args([
                    target.as_str(),
                    &format!("stat -c %Y {} 2>/dev/null || echo 'not found'", expanded),
                ])
                .output()
                .await;

            match output {
                Ok(out) => {
                    let ts = String::from_utf8_lossy(&out.stdout);
                    println!("vault@{}: {}", target, ts.trim());
                }
                Err(e) => {
                    println!("vault@{}: unreachable ({})", target, e);
                }
            }
        }
    }

    println!();

    // Machine status
    for (name, machine) in &config.machines {
        if machine.host == "localhost" {
            println!("machine/{}: local ({})", name, machine.role);
        } else {
            let output = Command::new("ssh")
                .args([
                    "-o",
                    "ConnectTimeout=3",
                    &machine.host,
                    "uptime -p 2>/dev/null || echo 'up'",
                ])
                .output()
                .await;

            match output {
                Ok(out) if out.status.success() => {
                    let uptime = String::from_utf8_lossy(&out.stdout);
                    println!(
                        "machine/{}: {} ({}) — {}",
                        name,
                        machine.host,
                        machine.role,
                        uptime.trim()
                    );
                }
                _ => {
                    println!("machine/{}: {} (unreachable)", name, machine.host);
                }
            }
        }
    }

    Ok(())
}
