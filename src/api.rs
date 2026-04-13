//! Local HTTP API for lifecycle observability.
//!
//! `aide api` starts axum at 127.0.0.1:7979.
//! Consumed by `aide dash`, `aide-skill aide serve`, and crossmem-rs.

use axum::{extract::Query, routing::get, Json, Router};
use serde::Deserialize;
use std::net::SocketAddr;

use crate::{db, registry};

/// Start the HTTP API server.
pub async fn serve(port: u16) -> anyhow::Result<()> {
    let app = Router::new()
        .route("/api/runs", get(get_runs))
        .route("/api/agents", get(get_agents))
        .route("/api/heartbeat", get(get_heartbeat))
        .route("/api/stats", get(get_stats))
        .route("/api/health", get(get_health))
        .route("/api/telemetry", get(get_telemetry));

    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    tracing::info!("aide API listening on http://{addr}");
    println!("aide API → http://{addr}");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

#[derive(Deserialize)]
struct RunsQuery {
    #[serde(default = "default_limit")]
    limit: usize,
}
fn default_limit() -> usize {
    30
}

async fn get_runs(Query(q): Query<RunsQuery>) -> Json<Vec<db::RunRow>> {
    match db::recent_runs(q.limit) {
        Ok(rows) => Json(rows),
        Err(_) => Json(vec![]),
    }
}

async fn get_agents() -> Json<Vec<AgentInfo>> {
    let agents = registry::list().unwrap_or_default();
    let infos: Vec<AgentInfo> = agents
        .iter()
        .map(|a| {
            let trigger = crate::aidefile::load(&std::path::PathBuf::from(
                shellexpand::tilde(&a.path).as_ref(),
            ))
            .map(|af| af.trigger.on.clone())
            .unwrap_or_else(|_| "error".into());
            AgentInfo {
                name: a.name.clone(),
                path: a.path.clone(),
                trigger,
            }
        })
        .collect();
    Json(infos)
}

#[derive(serde::Serialize)]
struct AgentInfo {
    name: String,
    path: String,
    trigger: String,
}

async fn get_heartbeat() -> Json<Option<db::Heartbeat>> {
    Json(db::last_heartbeat().unwrap_or(None))
}

async fn get_stats() -> Json<db::DailyStats> {
    Json(db::stats_today().unwrap_or(db::DailyStats {
        date: chrono::Utc::now().format("%Y-%m-%d").to_string(),
        total_runs: 0,
        successful: 0,
        failed: 0,
        total_tokens: 0,
        agents_used: vec![],
    }))
}

async fn get_health() -> Json<HealthResponse> {
    let hb = db::last_heartbeat().unwrap_or(None);
    let daemon_alive = hb.as_ref().map_or(false, |h| {
        // Check if heartbeat is within last 2 minutes
        if let Ok(ts) = chrono::DateTime::parse_from_rfc3339(&h.ts) {
            let age = chrono::Utc::now() - ts.with_timezone(&chrono::Utc);
            age.num_seconds() < 120
        } else {
            false
        }
    });
    Json(HealthResponse {
        version: env!("CARGO_PKG_VERSION").to_string(),
        daemon_alive,
        last_heartbeat: hb,
    })
}

#[derive(serde::Serialize)]
struct HealthResponse {
    version: String,
    daemon_alive: bool,
    last_heartbeat: Option<db::Heartbeat>,
}

async fn get_telemetry() -> Json<db::TelemetrySummary> {
    Json(db::telemetry_summary().unwrap_or(db::TelemetrySummary {
        total_runs: 0,
        avg_compression_ratio: 0.0,
        total_sub_agent_tokens: 0,
        total_frontier_wait_tokens: 0,
        total_frontier_dispatch_tokens: 0,
        tokens_saved: 0,
        savings_multiplier: 0.0,
    }))
}
