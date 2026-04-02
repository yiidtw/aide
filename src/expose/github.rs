use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::PathBuf;
use tracing::{error, info, warn};

use crate::agents::agentfile::AgentfileSpec;
use crate::agents::instance::{self, InstanceManager};

const GITHUB_API: &str = "https://api.github.com";
const POLL_INTERVAL_SECS: u64 = 300; // 5 minutes

/// Per-instance polling state, kept in memory across ticks.
struct InstanceState {
    etag: Option<String>,
    last_seen_issue: u64,
    last_seen_comments: HashMap<u64, u64>,
    seeded: bool,
}

/// Start a single GitHub Issues ticker that scans all instances every 300s.
///
/// Unlike per-instance pollers, this automatically picks up new instances
/// with `github_repo` without requiring a daemon restart.
pub fn start_github_issues_ticker(data_dir: String) {
    tokio::spawn(async move {
        let client = reqwest::Client::new();
        let mut states: HashMap<String, InstanceState> = HashMap::new();

        // Load token once (refreshed each tick in case vault changes)
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(POLL_INTERVAL_SECS));
        // First tick fires immediately — use it to seed
        interval.tick().await;

        info!(interval_secs = POLL_INTERVAL_SECS, "github issues ticker started");

        loop {
            let vault_env = load_vault_env().unwrap_or_default();
            let token = vault_env
                .iter()
                .find(|(k, _)| k == "GITHUB_TOKEN")
                .map(|(_, v)| v.clone());

            let Some(token) = token else {
                // No token — wait for next tick
                interval.tick().await;
                continue;
            };

            // Resolve authenticated GitHub username (for comment loop prevention)
            let gh_user = match client
                .get(format!("{}/user", GITHUB_API))
                .header("Authorization", format!("Bearer {}", token))
                .header("User-Agent", "aide.sh")
                .send()
                .await
            {
                Ok(resp) => resp.json::<serde_json::Value>().await
                    .ok()
                    .and_then(|v| v["login"].as_str().map(|s| s.to_string()))
                    .unwrap_or_default(),
                Err(_) => String::new(),
            };

            let mgr = InstanceManager::new(&data_dir);
            let instances = match mgr.list() {
                Ok(v) => v,
                Err(_) => {
                    interval.tick().await;
                    continue;
                }
            };

            for inst in &instances {
                // Ensure UUID exists (backfill for old instances)
                let _ = mgr.ensure_uuid(&inst.name);

                // Resolve github_repo from manifest or Agentfile
                let repo = match resolve_github_repo(&mgr, &inst.name) {
                    Some(r) => r,
                    None => continue,
                };

                let inst_dir = mgr.base_dir().join(&inst.name);

                // Load manifest for identity info
                let manifest = mgr.get(&inst.name).ok().flatten();

                // Get or create per-instance state
                let state = states.entry(inst.name.clone()).or_insert_with(|| {
                    info!(instance = %inst.name, repo = %repo, "github poller tracking new instance");
                    InstanceState {
                        etag: None,
                        last_seen_issue: 0,
                        last_seen_comments: HashMap::new(),
                        seeded: false,
                    }
                });

                // Seed on first encounter (don't process existing issues)
                if !state.seeded {
                    if let Ok(issues) = fetch_issues(&client, &repo, &token, &mut state.etag).await {
                        if let Some(max_id) = issues.iter().filter_map(|i| i["number"].as_u64()).max() {
                            state.last_seen_issue = max_id;
                            info!(instance = %inst.name, last_seen = state.last_seen_issue, "seeded from existing issues");
                        }
                    }
                    state.seeded = true;
                    continue; // Skip processing on seed tick
                }

                // Poll for new issues
                match fetch_issues(&client, &repo, &token, &mut state.etag).await {
                    Ok(issues) => {
                        for issue in &issues {
                            let number = match issue["number"].as_u64() {
                                Some(n) => n,
                                None => continue,
                            };

                            if number <= state.last_seen_issue {
                                continue;
                            }

                            let title = issue["title"].as_str().unwrap_or("");
                            let body = issue["body"].as_str().unwrap_or("");
                            let author = issue["user"]["login"].as_str().unwrap_or("unknown");

                            info!(
                                instance = %inst.name,
                                issue = number,
                                author = %author,
                                title = %title,
                                "new github issue"
                            );

                            // Routing check
                            let body_text = format!("{}\n{}", title, body);
                            let (my_machine, my_uuid_pfx) = get_identity(&manifest);
                            if !is_routed_to_us(&body_text, &my_machine, &my_uuid_pfx) {
                                state.last_seen_issue = state.last_seen_issue.max(number);
                                continue;
                            }

                            // Leader election: try to claim via 👀 reaction
                            if !try_claim_issue(&client, &repo, &token, number).await {
                                info!(instance = %inst.name, issue = number, "lost leader election, skipping");
                                state.last_seen_issue = state.last_seen_issue.max(number);
                                continue;
                            }

                            let _ = mgr.append_log(
                                &inst.name,
                                &format!("github-issue: #{} by {} — {}", number, author, title),
                            );

                            // Ack with identity
                            let ack_msg = format_ack(&inst.name, &manifest);
                            if let Err(e) = post_comment(&client, &repo, &token, number, &ack_msg).await {
                                warn!(error = %e, "failed to ack issue #{}", number);
                            }

                            // Execute
                            let query = if body.is_empty() {
                                title.to_string()
                            } else {
                                format!("{}\n{}", title, body)
                            };
                            let result = exec_agent(&inst.name, &inst_dir, &query);

                            match &result {
                                Ok(output) => {
                                    let _ = mgr.append_log(
                                        &inst.name,
                                        &format!("github-issue-result: #{} → ok", number),
                                    );
                                    let result_body = if output.trim().is_empty() { "(no output)".to_string() }
                                        else { truncate_for_comment(output) };
                                    if let Err(e) = post_comment(&client, &repo, &token, number, &result_body).await {
                                        error!(error = %e, "failed to post result on issue #{}", number);
                                    }
                                    // Commit memory changes back to repo
                                    if let Err(e) = commit_memory(&client, &repo, &token, &inst_dir, number).await {
                                        warn!(error = %e, "failed to commit memory for issue #{}", number);
                                    }
                                    // Only advance last_seen on success
                                    state.last_seen_issue = state.last_seen_issue.max(number);
                                }
                                Err(e) => {
                                    // LLM unavailable — do NOT advance last_seen, will retry next tick
                                    warn!(
                                        instance = %inst.name,
                                        issue = number,
                                        error = %e,
                                        "exec failed, will retry next tick"
                                    );
                                    let _ = mgr.append_log(
                                        &inst.name,
                                        &format!("github-issue-result: #{} → deferred ({})", number, e),
                                    );
                                    // Post a temporary comment so sender knows we're trying
                                    let _ = post_comment(
                                        &client, &repo, &token, number,
                                        &format!("⏳ Processing deferred — LLM unavailable, will retry.\n\n_{}_", e),
                                    ).await;
                                }
                            }
                        }
                    }
                    Err(e) => {
                        warn!(instance = %inst.name, error = %e, "github issues fetch failed");
                    }
                }

                // Poll for new comments
                if let Err(e) = poll_issue_comments(
                    &client, &mgr, &inst.name, &inst_dir, &repo, &token,
                    &mut state.last_seen_comments, &manifest, &gh_user,
                ).await {
                    warn!(instance = %inst.name, error = %e, "github comments poll failed");
                }
            }

            // Clean up states for removed instances
            let active_names: std::collections::HashSet<String> =
                instances.iter().map(|i| i.name.clone()).collect();
            states.retain(|name, _| active_names.contains(name));

            interval.tick().await;
        }
    });
}

/// Resolve github_repo from instance manifest or Agentfile expose.github.
fn resolve_github_repo(mgr: &InstanceManager, instance: &str) -> Option<String> {
    let manifest = mgr.get(instance).ok()??;
    if let Some(repo) = &manifest.github_repo {
        return Some(repo.clone());
    }
    let inst_dir = mgr.base_dir().join(instance);
    let spec = AgentfileSpec::load(&inst_dir).ok()?;
    spec.expose?.github.map(|gh| gh.repo)
}

/// Fetch open issues sorted by creation date (newest first).
/// Uses ETag for conditional requests — returns empty vec on 304.
async fn fetch_issues(
    client: &reqwest::Client,
    repo: &str,
    token: &str,
    etag: &mut Option<String>,
) -> Result<Vec<serde_json::Value>> {
    let url = format!(
        "{}/repos/{}/issues?state=open&sort=created&direction=desc&per_page=10",
        GITHUB_API, repo
    );

    let mut req = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", token))
        .header("User-Agent", "aide-agent")
        .header("Accept", "application/vnd.github+json");

    if let Some(etag_val) = etag {
        req = req.header("If-None-Match", etag_val.as_str());
    }

    let resp = req.send().await.context("github API request failed")?;

    if resp.status() == reqwest::StatusCode::NOT_MODIFIED {
        return Ok(Vec::new());
    }

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("github API error ({}): {}", status, body);
    }

    if let Some(new_etag) = resp.headers().get("etag") {
        if let Ok(val) = new_etag.to_str() {
            *etag = Some(val.to_string());
        }
    }

    let issues: Vec<serde_json::Value> = resp.json().await?;

    Ok(issues
        .into_iter()
        .filter(|i| i.get("pull_request").is_none())
        .collect())
}

/// Post a comment on an issue.
async fn post_comment(
    client: &reqwest::Client,
    repo: &str,
    token: &str,
    issue_number: u64,
    body: &str,
) -> Result<()> {
    let url = format!(
        "{}/repos/{}/issues/{}/comments",
        GITHUB_API, repo, issue_number
    );

    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", token))
        .header("User-Agent", "aide-agent")
        .header("Accept", "application/vnd.github+json")
        .json(&serde_json::json!({ "body": body }))
        .send()
        .await
        .context("failed to post comment")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("post comment failed ({}): {}", status, body);
    }

    Ok(())
}

/// Poll for new comments on open issues.
async fn poll_issue_comments(
    client: &reqwest::Client,
    mgr: &InstanceManager,
    instance: &str,
    inst_dir: &std::path::Path,
    repo: &str,
    token: &str,
    last_seen_comments: &mut HashMap<u64, u64>,
    manifest: &Option<crate::agents::instance::InstanceManifest>,
    gh_user: &str,
) -> Result<()> {
    let url = format!(
        "{}/repos/{}/issues?state=open&sort=updated&direction=desc&per_page=5",
        GITHUB_API, repo
    );

    let resp = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", token))
        .header("User-Agent", "aide-agent")
        .header("Accept", "application/vnd.github+json")
        .send()
        .await?;

    if !resp.status().is_success() {
        return Ok(());
    }

    let issues: Vec<serde_json::Value> = resp.json().await?;

    for issue in &issues {
        if issue.get("pull_request").is_some() {
            continue;
        }

        let number = match issue["number"].as_u64() {
            Some(n) => n,
            None => continue,
        };

        let comments_url = format!(
            "{}/repos/{}/issues/{}/comments?per_page=10&sort=created&direction=desc",
            GITHUB_API, repo, number
        );

        let comments_resp = client
            .get(&comments_url)
            .header("Authorization", format!("Bearer {}", token))
            .header("User-Agent", "aide-agent")
            .header("Accept", "application/vnd.github+json")
            .send()
            .await?;

        if !comments_resp.status().is_success() {
            continue;
        }

        let comments: Vec<serde_json::Value> = comments_resp.json().await?;
        let last_seen = last_seen_comments.get(&number).copied().unwrap_or(0);

        for comment in &comments {
            let comment_id = match comment["id"].as_u64() {
                Some(id) => id,
                None => continue,
            };

            if comment_id <= last_seen {
                continue;
            }

            let author = comment["user"]["login"].as_str().unwrap_or("");
            if author.ends_with("[bot]") || author == "github-actions[bot]" {
                last_seen_comments.insert(number, last_seen.max(comment_id));
                continue;
            }

            let body = comment["body"].as_str().unwrap_or("");
            if body.is_empty() || body.starts_with("🤖") {
                last_seen_comments.insert(number, last_seen.max(comment_id));
                continue;
            }

            // Skip comments from ourselves (same GITHUB_TOKEN user) to prevent comment loops
            if author == gh_user {
                last_seen_comments.insert(number, last_seen.max(comment_id));
                continue;
            }

            // Routing check
            let (my_machine, my_uuid_pfx) = get_identity(manifest);
            if !is_routed_to_us(body, &my_machine, &my_uuid_pfx) {
                last_seen_comments.insert(number, last_seen.max(comment_id));
                continue;
            }

            // Leader election: try to claim via 👀 reaction on comment
            if !try_claim_comment(client, repo, token, comment_id).await {
                info!(instance = %instance, issue = number, comment = comment_id, "lost comment leader election, skipping");
                last_seen_comments.insert(number, last_seen.max(comment_id));
                continue;
            }

            info!(
                instance = %instance,
                issue = number,
                comment = comment_id,
                author = %author,
                "new github comment"
            );
            let _ = mgr.append_log(
                instance,
                &format!("github-comment: #{} by {} — {}", number, author, truncate(body, 100)),
            );

            let ack_msg = format_ack(instance, manifest);
            let _ = post_comment(client, repo, token, number, &ack_msg).await;

            let result = exec_agent(instance, inst_dir, body);

            let result_body = match result {
                Ok(output) => {
                    let _ = mgr.append_log(
                        instance,
                        &format!("github-comment-result: #{}/{} → ok", number, comment_id),
                    );
                    if output.trim().is_empty() { "(no output)".to_string() }
                    else { truncate_for_comment(&output) }
                }
                Err(e) => {
                    let _ = mgr.append_log(
                        instance,
                        &format!("github-comment-result: #{}/{} → error: {}", number, comment_id, e),
                    );
                    format!("Error: {}", e)
                }
            };

            if let Err(e) = post_comment(client, repo, token, number, &result_body).await {
                error!(error = %e, "failed to post comment result");
            }

            // Commit memory changes
            if let Err(e) = commit_memory(client, repo, token, inst_dir, number).await {
                warn!(error = %e, "failed to commit memory for comment #{}/{}", number, comment_id);
            }

            last_seen_comments.insert(number, last_seen.max(comment_id));
        }

        if !last_seen_comments.contains_key(&number) {
            if let Some(max_id) = comments.iter().filter_map(|c| c["id"].as_u64()).max() {
                last_seen_comments.insert(number, max_id);
            }
        }
    }

    Ok(())
}

/// Execute agent skills via LLM routing.
fn exec_agent(
    instance: &str,
    inst_dir: &std::path::Path,
    query: &str,
) -> Result<String> {
    // Read persona (try occupation/persona.md first, fall back to persona.md)
    let persona_path = crate::agents::instance::resolve_path(inst_dir, "occupation/persona.md", "persona.md");
    let persona = std::fs::read_to_string(persona_path).unwrap_or_default();

    // Local skills from Agentfile
    let local_skills = AgentfileSpec::load(inst_dir)
        .ok()
        .map(|spec| spec.format_help(instance))
        .unwrap_or_default();

    // Global aide-skill catalog (injected so agent knows all available skills)
    let global_skills = crate::agents::enumerate_aide_skills();

    let skill_info = format!("{}{}", local_skills, global_skills);

    // Vault keys available to this agent
    let vault_env = load_vault_env().unwrap_or_default();
    let vault_info = if vault_env.is_empty() {
        String::new()
    } else {
        let keys: Vec<&str> = vault_env.iter().map(|(k, _)| k.as_str()).collect();
        format!("\n\n## Vault (available secrets)\n{}\n(injected automatically into skill env)", keys.join(", "))
    };

    // Read own memory files (cognition/memory/*.md)
    let memory = read_memory_dir(
        &crate::agents::instance::resolve_path(inst_dir, "cognition/memory", "memory"),
        "Memory"
    );

    // If this is an org router, also inject each member's memory
    let org_member_memory = read_org_member_memory(inst_dir, instance);

    let prompt = format!(
        "You are an autonomous agent running in DAEMON MODE (not interactive).\n\
         You have NO access to MCP tools, browser, or interactive UI.\n\
         Do NOT ask for permissions, do NOT mention MCP, do NOT request user interaction.\n\n\
         Actions (one per line):\n\
         - EXEC: <skill_name> [args]  — run a skill\n\
         - MEMORY: <filename.md> | <content>  — update a memory file (REQUIRES prior EXEC evidence)\n\
         - Or a direct text answer\n\n\
         MEMORY rules (STRICT):\n\
         - MEMORY: is ONLY allowed after a successful EXEC: that provides evidence\n\
         - The memory content MUST reference the evidence (what skill ran, what it returned)\n\
         - If you cannot produce evidence from an EXEC, you MUST NOT write to memory\n\
         - Never write 'submitted', 'done', 'completed' to memory without EXEC output proving it\n\
         - Memory writes are posted to the GitHub issue as an audit trail\n\n\
         ## Persona\n{}\n\n## Available Skills\n{}{}{}{}\n\n## Task\n{}",
        persona, skill_info, vault_info, memory, org_member_memory, query
    );

    let output = std::process::Command::new("claude")
        .args(["--allowedTools", "", "-p"])
        .arg(&prompt)
        .current_dir(inst_dir)
        .output();

    let response = match output {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).to_string(),
        _ => {
            let ollama = std::process::Command::new("ollama")
                .args(["run", "llama3.2:3b"])
                .arg(&prompt)
                .output();
            match ollama {
                Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).to_string(),
                _ => anyhow::bail!("No LLM available (claude/ollama)"),
            }
        }
    };

    let mut results = Vec::new();
    let mut has_exec = false;
    let mut memory_updates: Vec<(String, String)> = Vec::new(); // (filename, content)

    for line in response.lines() {
        let line = line.trim();
        if let Some(cmd) = line.strip_prefix("EXEC:") {
            has_exec = true;
            let cmd = cmd.trim();
            let parts: Vec<&str> = cmd.splitn(2, ' ').collect();
            let skill_name = parts[0];
            let args = if parts.len() > 1 { parts[1] } else { "" };

            let script = find_skill_script(inst_dir, skill_name);
            let exec_result = if let Some(script) = script {
                exec_skill_raw(&script, args, inst_dir, &vault_env)
            } else {
                tracing::info!(skill = skill_name, "local script not found, trying aide-skill");
                exec_aide_skill(skill_name, args, inst_dir, &vault_env)
            };
            match exec_result {
                Ok((_, stdout, stderr)) => {
                    if !stdout.trim().is_empty() { results.push(stdout.clone()); }
                    if !stderr.trim().is_empty() { results.push(format!("[stderr] {}", stderr)); }
                }
                Err(e) => results.push(format!("Error running {}: {}", skill_name, e)),
            }
        } else if let Some(mem_cmd) = line.strip_prefix("MEMORY:") {
            // MEMORY: <filename.md> | <content>
            if let Some((filename, content)) = mem_cmd.trim().split_once('|') {
                memory_updates.push((filename.trim().to_string(), content.trim().to_string()));
            }
        }
    }

    // Apply memory updates — only if we have EXEC evidence
    if !memory_updates.is_empty() {
        let evidence = if has_exec && !results.is_empty() {
            format!("\n\n<!-- evidence from EXEC -->\n{}", results.join("\n"))
        } else {
            String::new()
        };

        let mem_dir = crate::agents::instance::resolve_path(inst_dir, "cognition/memory", "memory");
        let _ = std::fs::create_dir_all(&mem_dir);

        for (filename, content) in &memory_updates {
            // Sanitize filename — only allow .md files, no path traversal
            let safe_name = std::path::Path::new(filename)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            if safe_name.is_empty() || !safe_name.ends_with(".md") { continue; }

            let mem_path = mem_dir.join(&safe_name);
            let entry = format!(
                "\n\n## Update ({})\n{}{}\n",
                chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ"),
                content,
                evidence
            );

            // Append to existing file or create new
            let existing = std::fs::read_to_string(&mem_path).unwrap_or_default();
            let _ = std::fs::write(&mem_path, format!("{}{}", existing, entry));

            results.push(format!("✅ memory updated: {} (evidence attached)", safe_name));
            tracing::info!(instance, file = %safe_name, "memory updated with evidence");
        }
    }

    if has_exec || !memory_updates.is_empty() { Ok(results.join("\n")) } else { Ok(response) }
}

fn exec_skill_raw(
    script: &std::path::Path,
    args: &str,
    working_dir: &std::path::Path,
    env: &[(String, String)],
) -> Result<(i32, String, String)> {
    let ext = script.extension().and_then(|e| e.to_str()).unwrap_or("sh");
    let mut cmd = if ext == "ts" {
        let bun = crate::find_or_install_bun()?;
        let mut c = std::process::Command::new(bun);
        c.arg("run");
        c.arg(script);
        c
    } else {
        let mut c = std::process::Command::new("bash");
        c.arg(script);
        c
    };
    if !args.is_empty() {
        for arg in args.split_whitespace() {
            cmd.arg(arg);
        }
    }
    cmd.current_dir(working_dir);
    cmd.env("AIDE_INSTANCE_DIR", working_dir);
    for (k, v) in env {
        cmd.env(k, v);
    }
    let output = cmd
        .output()
        .with_context(|| format!("failed to execute: {}", script.display()))?;
    Ok((
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    ))
}

/// Fallback: execute a skill via aide-skill CLI.
fn exec_aide_skill(
    skill_name: &str,
    args: &str,
    working_dir: &std::path::Path,
    env: &[(String, String)],
) -> Result<(i32, String, String)> {
    let mut cmd = std::process::Command::new("aide-skill");
    cmd.arg(skill_name);
    if !args.is_empty() {
        for a in args.split_whitespace() {
            cmd.arg(a);
        }
    }
    cmd.current_dir(working_dir);
    cmd.env("AIDE_INSTANCE_DIR", working_dir);
    for (k, v) in env {
        cmd.env(k, v);
    }
    let output = cmd
        .output()
        .with_context(|| format!("aide-skill {} not found or failed", skill_name))?;
    Ok((
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    ))
}

/// Find a skill script, trying occupation/skills/ first, then skills/.
fn find_skill_script(inst_dir: &std::path::Path, skill_name: &str) -> Option<std::path::PathBuf> {
    for dir in &["occupation/skills", "skills"] {
        for ext in &["ts", "sh"] {
            let path = inst_dir.join(dir).join(format!("{}.{}", skill_name, ext));
            if path.exists() {
                return Some(path);
            }
        }
    }
    None
}

/// Read all *.md files from a memory directory into a formatted string.
fn read_memory_dir(dir: &std::path::Path, heading: &str) -> String {
    if !dir.exists() { return String::new(); }
    let mut mem = String::new();
    if let Ok(mut entries) = std::fs::read_dir(dir) {
        let mut paths: Vec<_> = entries.filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map(|x| x == "md").unwrap_or(false))
            .collect();
        paths.sort_by_key(|e| e.file_name());
        for entry in paths {
            if let Ok(content) = std::fs::read_to_string(entry.path()) {
                let name = entry.path().file_stem().unwrap_or_default().to_string_lossy().to_string();
                mem.push_str(&format!("### {}\n{}\n\n", name, content));
            }
        }
    }
    if mem.is_empty() { String::new() } else { format!("\n\n## {}\n{}", heading, mem) }
}

/// If this instance is an org router, read each member's cognition/memory/*.md.
fn read_org_member_memory(inst_dir: &std::path::Path, instance: &str) -> String {
    // inst_dir is e.g. ~/.aide/instances/ntu.yiidtw — parent is instances/
    let instances_dir = match inst_dir.parent() {
        Some(p) => p,
        None => return String::new(),
    };

    // Read own instance.toml to check if router
    let own_toml = inst_dir.join("cognition/instance.toml");
    let content = match std::fs::read_to_string(&own_toml) {
        Ok(c) => c,
        Err(_) => return String::new(),
    };

    // Extract org and org_router fields
    let org = content.lines()
        .find(|l| l.trim_start().starts_with("org ="))
        .and_then(|l| l.split_once('='))
        .map(|(_, v)| v.trim().trim_matches('"').to_string());
    let org_router = content.lines()
        .find(|l| l.trim_start().starts_with("org_router ="))
        .and_then(|l| l.split_once('='))
        .map(|(_, v)| v.trim().trim_matches('"').to_string());

    // Only proceed if I am the router
    if org_router.as_deref() != Some(instance) { return String::new(); }
    let org = match org { Some(o) => o, None => return String::new() };

    // Scan sibling instances in same org
    let entries = match std::fs::read_dir(instances_dir) {
        Ok(e) => e,
        Err(_) => return String::new(),
    };

    let mut sections = Vec::new();
    for entry in entries.filter_map(|e| e.ok()) {
        let member_name = entry.file_name().to_string_lossy().to_string();
        if member_name == instance { continue; }
        let member_toml = entry.path().join("cognition/instance.toml");
        if let Ok(mc) = std::fs::read_to_string(&member_toml) {
            let member_org = mc.lines()
                .find(|l| l.trim_start().starts_with("org ="))
                .and_then(|l| l.split_once('='))
                .map(|(_, v)| v.trim().trim_matches('"').to_string());
            if member_org.as_deref() != Some(org.as_str()) { continue; }
        } else { continue; }

        let mem = read_memory_dir(
            &entry.path().join("cognition/memory"),
            &member_name,
        );
        if !mem.is_empty() {
            sections.push(format!("### Member: {}{}", member_name, mem));
        }
    }

    if sections.is_empty() { String::new() }
    else { format!("\n\n## Org Member Context\n{}", sections.join("\n")) }
}

pub fn github_vault_env() -> Vec<(String, String)> {
    load_vault_env().unwrap_or_default()
}

fn load_vault_env() -> Result<Vec<(String, String)>> {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let aide_home = PathBuf::from(&home).join(".aide");
    let vault_repo_path = PathBuf::from(&home).join("claude_projects/aide-vault/vault.age");
    let legacy_path = aide_home.join("vault.age");
    let vault_path = if vault_repo_path.exists() { vault_repo_path } else { legacy_path };
    if !vault_path.exists() { return Ok(Vec::new()); }
    let identity_path = aide_home.join("vault.key");
    if !identity_path.exists() { return Ok(Vec::new()); }

    let output = std::process::Command::new("age")
        .args(["-d", "-i"])
        .arg(&identity_path)
        .arg(&vault_path)
        .output()?;

    if !output.status.success() { return Ok(Vec::new()); }

    let content = String::from_utf8_lossy(&output.stdout);
    let mut vars = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') { continue; }
        let line = line.strip_prefix("export ").unwrap_or(line);
        if let Some((key, val)) = line.split_once('=') {
            let val = val.trim_matches('"').trim_matches('\'');
            vars.push((key.to_string(), val.to_string()));
        }
    }
    Ok(vars)
}

/// Commit changes (memory, skills, etc.) to the GitHub repo via in-place git.
///
/// If the instance dir is a git repo, stages all changes, commits, and pushes.
/// This replaces the old Contents API approach — the instance dir IS the repo.
async fn commit_memory(
    _client: &reqwest::Client,
    _repo: &str,
    _token: &str,
    inst_dir: &std::path::Path,
    issue_number: u64,
) -> Result<()> {
    let message = format!("memory: issue #{}", issue_number);
    if let Some(summary) = crate::agents::commit::auto_commit_instance(inst_dir, &message) {
        info!(dir = %inst_dir.display(), "memory committed: {}", summary.lines().next().unwrap_or("ok"));
    }
    Ok(())
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max { s.to_string() }
    else {
        let mut t: String = s.chars().take(max).collect();
        t.push_str("...");
        t
    }
}

/// Try to claim an issue by adding 👀 reaction.
/// Returns true if this machine wins the leader election.
async fn try_claim_issue(
    client: &reqwest::Client,
    repo: &str,
    token: &str,
    issue_number: u64,
) -> bool {
    let url = format!("{}/repos/{}/issues/{}/reactions", GITHUB_API, repo, issue_number);
    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", token))
        .header("User-Agent", "aide-agent")
        .header("Accept", "application/vnd.github+json")
        .json(&serde_json::json!({ "content": "eyes" }))
        .send()
        .await;

    match resp {
        Ok(r) if r.status().is_success() || r.status() == reqwest::StatusCode::CREATED => {
            // Successfully added reaction. Wait briefly then check if we were first.
            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

            let reactions_url = format!("{}/repos/{}/issues/{}/reactions", GITHUB_API, repo, issue_number);
            let reactions_resp = client
                .get(&reactions_url)
                .header("Authorization", format!("Bearer {}", token))
                .header("User-Agent", "aide-agent")
                .header("Accept", "application/vnd.github+json")
                .send()
                .await;

            match reactions_resp {
                Ok(r) if r.status().is_success() => {
                    let reactions: Vec<serde_json::Value> = r.json().await.unwrap_or_default();
                    let eyes: Vec<&serde_json::Value> = reactions.iter()
                        .filter(|r| r["content"].as_str() == Some("eyes"))
                        .collect();
                    // If only one 👀, we won. If multiple, earliest wins (we arrived first).
                    eyes.len() <= 1
                }
                _ => true, // Can't check, proceed anyway
            }
        }
        Ok(r) if r.status() == reqwest::StatusCode::UNPROCESSABLE_ENTITY => {
            // 422 = reaction already exists, someone else claimed it
            false
        }
        _ => false,
    }
}

/// Try to claim a comment by adding 👀 reaction to it.
async fn try_claim_comment(
    client: &reqwest::Client,
    repo: &str,
    token: &str,
    comment_id: u64,
) -> bool {
    let url = format!("{}/repos/{}/issues/comments/{}/reactions", GITHUB_API, repo, comment_id);
    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", token))
        .header("User-Agent", "aide-agent")
        .header("Accept", "application/vnd.github+json")
        .json(&serde_json::json!({ "content": "eyes" }))
        .send()
        .await;

    match resp {
        Ok(r) if r.status().is_success() || r.status() == reqwest::StatusCode::CREATED => true,
        _ => false,
    }
}

/// Format ack message with instance identity.
fn format_ack(instance_name: &str, manifest: &Option<crate::agents::instance::InstanceManifest>) -> String {
    let (machine, uuid_pfx) = get_identity(manifest);
    format!("🤖 {}({}:{}) received, processing...", instance_name, machine, uuid_pfx)
}

/// Extract machine_id and uuid_prefix from a manifest.
fn get_identity(manifest: &Option<crate::agents::instance::InstanceManifest>) -> (String, String) {
    match manifest {
        Some(m) => {
            let machine = m.machine_id.as_deref().unwrap_or("unknown").to_string();
            let uuid = m.uuid.as_deref().unwrap_or("????");
            let prefix = instance::uuid_prefix(uuid);
            (machine, prefix)
        }
        None => ("unknown".to_string(), "????".to_string()),
    }
}

/// Check if a message is routed to this instance.
/// Returns true if no routing specified, or if routed to us.
fn is_routed_to_us(
    text: &str,
    machine_id: &str,
    uuid_prefix: &str,
) -> bool {
    let at_machine = format!("@{}", machine_id);
    let at_uuid = format!("@{}", uuid_prefix);

    // If no @ routing in text, anyone can handle it
    if !text.contains('@') {
        return true;
    }

    text.contains(&at_machine) || text.contains(&at_uuid)
}

fn truncate_for_comment(s: &str) -> String {
    const MAX_COMMENT_LEN: usize = 60000;
    if s.len() <= MAX_COMMENT_LEN { s.to_string() }
    else {
        let mut t: String = s.chars().take(MAX_COMMENT_LEN).collect();
        t.push_str("\n\n...(truncated)");
        t
    }
}
