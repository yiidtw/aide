use anyhow::Result;
use axum::extract::{Path, Query, State};
use axum::http::{header, StatusCode};
use axum::response::{Html, IntoResponse, Json};
use axum::routing::get;
use axum::Router;
use rust_embed::Embed;
use serde::Deserialize;
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;

use crate::agents::agentfile::AgentfileSpec;
use crate::agents::instance::InstanceManager;

#[derive(Embed)]
#[folder = "src/dashboard/static/"]
struct Assets;

struct AppState {
    mgr: InstanceManager,
}

/// Start the dashboard HTTP server on the given port.
/// Returns a future that runs until shutdown.
pub async fn serve(data_dir: &str, port: u16) -> Result<()> {
    let mgr = InstanceManager::new(data_dir);
    let state = Arc::new(AppState { mgr });

    let app = Router::new()
        .route("/", get(index_handler))
        .route("/api/instances", get(api_instances))
        .route("/api/instance/{name}", get(api_instance_detail))
        .route("/api/logs/{name}", get(api_logs))
        .route("/api/stats/{name}", get(api_stats))
        .fallback(get(static_handler))
        .with_state(state);

    let addr = format!("0.0.0.0:{}", port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!(port = port, "dashboard serving at http://localhost:{}", port);

    axum::serve(listener, app).await?;
    Ok(())
}

/// Spawn dashboard as a background tokio task (for integration into daemon).
pub fn spawn_dashboard(data_dir: String, port: u16) {
    tokio::spawn(async move {
        if let Err(e) = serve(&data_dir, port).await {
            tracing::error!(error = %e, "dashboard server error");
        }
    });
}

// ─── Handlers ───

async fn index_handler() -> impl IntoResponse {
    match Assets::get("index.html") {
        Some(content) => Html(
            std::str::from_utf8(content.data.as_ref())
                .unwrap_or("")
                .to_string(),
        )
        .into_response(),
        None => (StatusCode::NOT_FOUND, "index.html not found").into_response(),
    }
}

async fn static_handler(uri: axum::http::Uri) -> impl IntoResponse {
    let path = uri.path().trim_start_matches('/');
    match Assets::get(path) {
        Some(content) => {
            let mime = mime_guess::from_path(path)
                .first_or_octet_stream()
                .to_string();
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, mime)],
                content.data.to_vec(),
            )
                .into_response()
        }
        None => (StatusCode::NOT_FOUND, "not found").into_response(),
    }
}

async fn api_instances(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    match state.mgr.list() {
        Ok(instances) => {
            let list: Vec<_> = instances
                .iter()
                .map(|inst| {
                    json!({
                        "name": inst.name,
                        "agent_type": inst.agent_type,
                        "status": format!("{}", inst.status),
                        "email": inst.email,
                        "role": inst.role,
                        "cron_count": inst.cron_count,
                        "last_activity": inst.last_activity,
                    })
                })
                .collect();
            Json(json!({ "instances": list })).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

async fn api_instance_detail(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    let manifest = match state.mgr.get(&name) {
        Ok(Some(m)) => m,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": format!("instance '{}' not found", name) })),
            )
                .into_response()
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
                .into_response()
        }
    };

    let inst_dir = state.mgr.base_dir().join(&name);

    // Load Agentfile for skill metadata
    let (version, description, author, skills) = if let Ok(spec) = AgentfileSpec::load(&inst_dir) {
        let mut skill_list: Vec<serde_json::Value> = spec
            .skills
            .iter()
            .map(|(sname, sdef)| {
                json!({
                    "name": sname,
                    "type": if sdef.script.is_some() { "script" } else { "prompt" },
                    "description": sdef.description,
                    "usage": sdef.usage,
                    "schedule": sdef.schedule,
                    "env": sdef.env,
                })
            })
            .collect();
        skill_list.sort_by(|a, b| {
            a["name"]
                .as_str()
                .unwrap_or("")
                .cmp(b["name"].as_str().unwrap_or(""))
        });
        (
            spec.agent.version.clone(),
            spec.agent.description.clone(),
            spec.agent.author.clone(),
            skill_list,
        )
    } else {
        (String::new(), None, None, Vec::new())
    };

    // Cron entries
    let cron: Vec<serde_json::Value> = manifest
        .cron
        .iter()
        .map(|c| {
            json!({
                "schedule": c.schedule,
                "skill": c.skill,
                "last_run": c.last_run.map(|t| t.format("%Y-%m-%d %H:%M").to_string()),
            })
        })
        .collect();

    Json(json!({
        "name": manifest.name,
        "agent_type": manifest.agent_type,
        "version": version,
        "description": description,
        "author": author,
        "email": manifest.email,
        "role": manifest.role,
        "created_at": manifest.created_at.to_rfc3339(),
        "skills": skills,
        "cron": cron,
    }))
    .into_response()
}

#[derive(Deserialize)]
struct LogsQuery {
    tail: Option<usize>,
}

async fn api_logs(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Query(query): Query<LogsQuery>,
) -> impl IntoResponse {
    let tail = query.tail.unwrap_or(100);

    // Verify instance exists
    match state.mgr.get(&name) {
        Ok(Some(_)) => {}
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": format!("instance '{}' not found", name) })),
            )
                .into_response()
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
                .into_response()
        }
    }

    match state.mgr.read_logs(&name, tail) {
        Ok(logs) => Json(json!({ "logs": logs })).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

async fn api_stats(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    // Verify instance exists
    match state.mgr.get(&name) {
        Ok(Some(_)) => {}
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": format!("instance '{}' not found", name) })),
            )
                .into_response()
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
                .into_response()
        }
    }

    let logs = match state.mgr.read_logs(&name, 10000) {
        Ok(l) => l,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
                .into_response()
        }
    };

    let mut total_execs: u64 = 0;
    let mut cli_count: u64 = 0;
    let mut mcp_count: u64 = 0;

    // skill_name -> (count, success, fail)
    let mut by_skill: HashMap<String, (u64, u64, u64)> = HashMap::new();

    for line in &logs {
        // Check for exec-result or mcp-exec-result
        let (is_mcp, rest) = if let Some(pos) = line.find("mcp-exec-result: ") {
            (true, &line[pos + "mcp-exec-result: ".len()..])
        } else if let Some(pos) = line.find("exec-result: ") {
            (false, &line[pos + "exec-result: ".len()..])
        } else {
            continue;
        };

        // Parse: <skill> → ok/FAILED
        let arrow_pos = match rest.find(" → ") {
            Some(p) => p,
            None => {
                // Try ASCII arrow as fallback
                match rest.find(" -> ") {
                    Some(p) => p,
                    None => continue,
                }
            }
        };

        let skill_full = &rest[..arrow_pos];
        // Take the first word as the skill name
        let skill_name = skill_full.split_whitespace().next().unwrap_or(skill_full);
        let after_arrow = if rest[arrow_pos..].starts_with(" → ") {
            &rest[arrow_pos + " → ".len()..]
        } else {
            &rest[arrow_pos + " -> ".len()..]
        };

        let is_success = after_arrow.starts_with("ok");

        total_execs += 1;
        if is_mcp {
            mcp_count += 1;
        } else {
            cli_count += 1;
        }

        let entry = by_skill.entry(skill_name.to_string()).or_insert((0, 0, 0));
        entry.0 += 1;
        if is_success {
            entry.1 += 1;
        } else {
            entry.2 += 1;
        }
    }

    let skill_map: serde_json::Map<String, serde_json::Value> = by_skill
        .into_iter()
        .map(|(name, (count, success, fail))| {
            (
                name,
                json!({ "count": count, "success": success, "fail": fail }),
            )
        })
        .collect();

    Json(json!({
        "total_execs": total_execs,
        "by_skill": skill_map,
        "by_source": { "cli": cli_count, "mcp": mcp_count },
    }))
    .into_response()
}
