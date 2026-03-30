use std::path::Path;

// ─── Git helpers ────────────────────────────────────────────────

fn git_output(inst_dir: &Path, args: &[&str]) -> Result<String, String> {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(inst_dir)
        .output()
        .map_err(|e| e.to_string())?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

fn git_ok(inst_dir: &Path, args: &[&str]) -> bool {
    std::process::Command::new("git")
        .args(args)
        .current_dir(inst_dir)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Commit summary with file counts by category.
struct CommitSummary {
    occ_count: usize,
    cog_count: usize,
    other_count: usize,
    push_ok: bool,
    sanity: String,
}

impl std::fmt::Display for CommitSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let total = self.occ_count + self.cog_count + self.other_count;
        write!(
            f,
            "committed: {} files ({} occupation, {} cognition{})\npushed: {}\n{}",
            total,
            self.occ_count,
            self.cog_count,
            if self.other_count > 0 {
                format!(", {} other", self.other_count)
            } else {
                String::new()
            },
            if self.push_ok { "ok" } else { "FAILED" },
            self.sanity,
        )
    }
}

fn push_and_verify(inst_dir: &Path) -> (bool, String) {
    let push_ok = git_ok(inst_dir, &["push"]);
    let sanity = if push_ok {
        git_ok(inst_dir, &["fetch", "origin", "--quiet"]);
        let local_head = git_output(inst_dir, &["rev-parse", "HEAD"]).unwrap_or_default();
        let remote_head =
            git_output(inst_dir, &["rev-parse", "origin/main"]).unwrap_or_default();
        if !local_head.is_empty() && local_head == remote_head {
            let short = &local_head[..7.min(local_head.len())];
            format!("sanity: HEAD == origin/main ({})", short)
        } else {
            format!(
                "sanity: MISMATCH local={} remote={}",
                &local_head[..7.min(local_head.len())],
                &remote_head[..7.min(remote_head.len())]
            )
        }
    } else {
        "sanity: push failed, remote not updated".to_string()
    };
    (push_ok, sanity)
}

fn count_staged(inst_dir: &Path) -> (usize, usize, usize) {
    let diff_stat =
        git_output(inst_dir, &["diff", "--cached", "--name-only"]).unwrap_or_default();
    let mut occ = 0usize;
    let mut cog = 0usize;
    let mut other = 0usize;
    for line in diff_stat.lines() {
        if line.starts_with("occupation/") {
            occ += 1;
        } else if line.starts_with("cognition/") {
            cog += 1;
        } else {
            other += 1;
        }
    }
    (occ, cog, other)
}

// ─── Public API ─────────────────────────────────────────────────

/// Auto-commit and push ALL changes in an instance directory.
/// Used by `aide exec`, `aide commit`, MCP `aide_exec` / `aide_commit`.
pub fn auto_commit_instance(inst_dir: &Path, message: &str) -> Option<String> {
    if !inst_dir.join(".git").exists() {
        return None;
    }

    // Stage all changes
    git_ok(inst_dir, &["add", "-A"]);

    // Check if there are staged changes
    if git_ok(inst_dir, &["diff", "--cached", "--quiet"]) {
        return None; // nothing to commit
    }

    let (occ_count, cog_count, other_count) = count_staged(inst_dir);

    if !git_ok(inst_dir, &["commit", "-m", message]) {
        tracing::warn!("auto-commit failed for {}", inst_dir.display());
        return None;
    }

    let (push_ok, sanity) = push_and_verify(inst_dir);
    if !push_ok {
        tracing::warn!("auto-push failed for {}", inst_dir.display());
    }

    Some(
        CommitSummary {
            occ_count,
            cog_count,
            other_count,
            push_ok,
            sanity,
        }
        .to_string(),
    )
}

/// Commit only `cognition/` changes (memory, logs) and push.
/// Used by the daily cron commit. Does NOT touch `occupation/` or other files.
pub fn commit_cognition(inst_dir: &Path) -> Option<String> {
    if !inst_dir.join(".git").exists() {
        return None;
    }

    // Only stage cognition/
    let cognition_dir = inst_dir.join("cognition");
    if !cognition_dir.exists() {
        return None;
    }

    git_ok(inst_dir, &["add", "cognition/"]);

    // Check if there are staged changes
    if git_ok(inst_dir, &["diff", "--cached", "--quiet"]) {
        return None; // nothing to commit
    }

    let (occ_count, cog_count, other_count) = count_staged(inst_dir);

    if !git_ok(inst_dir, &["commit", "-m", "daily: cognition sync"]) {
        tracing::warn!("daily cognition commit failed for {}", inst_dir.display());
        return None;
    }

    let (push_ok, sanity) = push_and_verify(inst_dir);
    if !push_ok {
        tracing::warn!("daily cognition push failed for {}", inst_dir.display());
    }

    Some(
        CommitSummary {
            occ_count,
            cog_count,
            other_count,
            push_ok,
            sanity,
        }
        .to_string(),
    )
}

/// Commit cognition/ for all git-backed instances. Returns per-instance summaries.
///
/// Note: `daily_commit_all` is integration-tested via the daemon's daily ticker.
/// Unit tests for `commit_cognition` and `auto_commit_instance` are in the `tests` module below.
pub fn daily_commit_all(data_dir: &str) -> Vec<(String, String)> {
    let mgr = super::instance::InstanceManager::new(data_dir);
    let instances = match mgr.list() {
        Ok(v) => v,
        Err(e) => {
            tracing::error!(error = %e, "daily commit: failed to list instances");
            return Vec::new();
        }
    };

    let mut results = Vec::new();
    for inst in &instances {
        let inst_dir = mgr.base_dir().join(&inst.name);
        if let Some(summary) = commit_cognition(&inst_dir) {
            tracing::info!(instance = %inst.name, "daily cognition committed");
            let _ = mgr.append_log(&inst.name, "daily: cognition sync committed");
            results.push((inst.name.clone(), summary));
        }
    }
    results
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Create a temp dir with `git init`, configure user, and make an initial commit.
    fn setup_git_repo() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();

        git_ok(dir, &["init", "-b", "main"]);
        git_ok(dir, &["config", "user.email", "test@test.com"]);
        git_ok(dir, &["config", "user.name", "Test"]);

        // Create directory structure
        fs::create_dir_all(dir.join("cognition/memory")).unwrap();
        fs::create_dir_all(dir.join("occupation/skills")).unwrap();

        // Initial commit so HEAD exists
        fs::write(dir.join(".gitignore"), "").unwrap();
        git_ok(dir, &["add", "-A"]);
        git_ok(dir, &["commit", "-m", "init"]);

        tmp
    }

    #[test]
    fn test_no_git_returns_none() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(auto_commit_instance(tmp.path(), "test").is_none());
        assert!(commit_cognition(tmp.path()).is_none());
    }

    #[test]
    fn test_no_changes_returns_none() {
        let tmp = setup_git_repo();
        assert!(auto_commit_instance(tmp.path(), "test").is_none());
        assert!(commit_cognition(tmp.path()).is_none());
    }

    #[test]
    fn test_auto_commit_stages_all() {
        let tmp = setup_git_repo();
        let dir = tmp.path();

        // Write files in both cognition and occupation
        fs::write(dir.join("cognition/memory/note.md"), "remember this").unwrap();
        fs::write(dir.join("occupation/skills/hello.sh"), "echo hi").unwrap();

        let summary = auto_commit_instance(dir, "exec: hello").unwrap();
        assert!(summary.contains("1 occupation"));
        assert!(summary.contains("1 cognition"));

        // Verify both files are committed
        let log = git_output(dir, &["log", "--oneline", "-1"]).unwrap();
        assert!(log.contains("exec: hello"));

        // Verify working tree is clean
        assert!(git_ok(dir, &["diff", "--quiet"]));
        assert!(auto_commit_instance(dir, "again").is_none());
    }

    #[test]
    fn test_commit_cognition_only_stages_cognition() {
        let tmp = setup_git_repo();
        let dir = tmp.path();

        // Write files in both cognition and occupation
        fs::write(dir.join("cognition/memory/note.md"), "remember this").unwrap();
        fs::write(dir.join("occupation/skills/hello.sh"), "echo hi").unwrap();

        let summary = commit_cognition(dir).unwrap();
        // Only cognition should be committed
        assert!(summary.contains("1 cognition"));
        assert!(summary.contains("0 occupation"));

        // Verify occupation file is still untracked
        let status = git_output(dir, &["status", "--porcelain"]).unwrap();
        assert!(status.contains("occupation/"));

        // Verify cognition file IS committed
        let log = git_output(dir, &["log", "--oneline", "-1"]).unwrap();
        assert!(log.contains("daily: cognition sync"));
    }

    #[test]
    fn test_commit_cognition_no_cognition_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();

        git_ok(dir, &["init", "-b", "main"]);
        git_ok(dir, &["config", "user.email", "test@test.com"]);
        git_ok(dir, &["config", "user.name", "Test"]);
        fs::write(dir.join(".gitignore"), "").unwrap();
        git_ok(dir, &["add", "-A"]);
        git_ok(dir, &["commit", "-m", "init"]);

        // No cognition/ dir exists
        assert!(commit_cognition(dir).is_none());
    }

    #[test]
    fn test_commit_summary_display() {
        let summary = CommitSummary {
            occ_count: 2,
            cog_count: 3,
            other_count: 0,
            push_ok: false,
            sanity: "sanity: push failed, remote not updated".to_string(),
        };
        let s = summary.to_string();
        assert!(s.contains("5 files"));
        assert!(s.contains("2 occupation"));
        assert!(s.contains("3 cognition"));
        assert!(s.contains("pushed: FAILED"));
    }

    #[test]
    fn test_count_staged_categorizes_correctly() {
        let tmp = setup_git_repo();
        let dir = tmp.path();

        fs::write(dir.join("cognition/memory/a.md"), "a").unwrap();
        fs::write(dir.join("cognition/memory/b.md"), "b").unwrap();
        fs::write(dir.join("occupation/skills/x.sh"), "x").unwrap();
        fs::write(dir.join("README.md"), "hello").unwrap();

        git_ok(dir, &["add", "-A"]);

        let (occ, cog, other) = count_staged(dir);
        assert_eq!(occ, 1);
        assert_eq!(cog, 2);
        assert_eq!(other, 1);
    }
}
