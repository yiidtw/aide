//! SQLite state store for lifecycle observability.
//!
//! `~/.aide/state.db` — persistent across daemon restarts.
//! Read by HTTP API, written by runner + daemon.

use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use crate::registry;

static DB: OnceLock<Mutex<Connection>> = OnceLock::new();

fn db_path() -> PathBuf {
    registry::aide_dir().join("state.db")
}

/// Get or initialize the shared database connection.
pub fn conn() -> &'static Mutex<Connection> {
    DB.get_or_init(|| {
        let path = db_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let c = Connection::open(&path).expect("Failed to open state.db");
        migrate(&c).expect("Failed to migrate state.db");
        Mutex::new(c)
    })
}

fn migrate(c: &Connection) -> Result<()> {
    c.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS runs (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            agent       TEXT NOT NULL,
            issue       TEXT,
            trigger     TEXT NOT NULL DEFAULT 'dispatch',
            task_preview TEXT,
            started_at  TEXT NOT NULL,
            finished_at TEXT,
            success     INTEGER,
            status      TEXT,
            tokens_used INTEGER DEFAULT 0,
            retries     INTEGER DEFAULT 0,
            summary     TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_runs_agent ON runs(agent);
        CREATE INDEX IF NOT EXISTS idx_runs_started ON runs(started_at);

        CREATE TABLE IF NOT EXISTS heartbeats (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            daemon_pid  INTEGER NOT NULL,
            ts          TEXT NOT NULL,
            agents_count INTEGER DEFAULT 0,
            uptime_secs INTEGER DEFAULT 0
        );

        CREATE TABLE IF NOT EXISTS dispatch_telemetry (
            id                      INTEGER PRIMARY KEY AUTOINCREMENT,
            run_id                  INTEGER REFERENCES runs(id),
            frontier_dispatch_tokens INTEGER DEFAULT 0,
            frontier_wait_tokens    INTEGER DEFAULT 0,
            sub_agent_tokens        INTEGER DEFAULT 0,
            compression_ratio       REAL
        );
        CREATE INDEX IF NOT EXISTS idx_telemetry_run ON dispatch_telemetry(run_id);
        ",
    )
    .context("Schema migration failed")?;
    Ok(())
}

// ── Run records ──

/// Insert a new run record when a dispatch starts. Returns the row id.
pub fn insert_run(agent: &str, issue: &str, task_preview: &str) -> Result<i64> {
    let c = conn().lock().unwrap();
    c.execute(
        "INSERT INTO runs (agent, issue, task_preview, started_at) VALUES (?1, ?2, ?3, ?4)",
        params![agent, issue, task_preview, crate::events::now()],
    )?;
    Ok(c.last_insert_rowid())
}

/// Update a run record when it finishes.
pub fn finish_run(
    run_id: i64,
    success: bool,
    status: &str,
    tokens_used: u64,
    retries: u32,
    summary: &str,
) -> Result<()> {
    let c = conn().lock().unwrap();
    c.execute(
        "UPDATE runs SET finished_at = ?1, success = ?2, status = ?3, tokens_used = ?4, retries = ?5, summary = ?6 WHERE id = ?7",
        params![
            crate::events::now(),
            success as i32,
            status,
            tokens_used as i64,
            retries as i32,
            summary,
            run_id,
        ],
    )?;
    Ok(())
}

/// Recent runs, newest first.
pub fn recent_runs(limit: usize) -> Result<Vec<RunRow>> {
    let c = conn().lock().unwrap();
    let mut stmt = c.prepare(
        "SELECT id, agent, issue, trigger, task_preview, started_at, finished_at, success, status, tokens_used, retries, summary
         FROM runs ORDER BY id DESC LIMIT ?1",
    )?;
    let rows = stmt
        .query_map(params![limit as i64], |row| {
            Ok(RunRow {
                id: row.get(0)?,
                agent: row.get(1)?,
                issue: row.get(2)?,
                trigger: row.get(3)?,
                task_preview: row.get(4)?,
                started_at: row.get(5)?,
                finished_at: row.get(6)?,
                success: row.get(7)?,
                status: row.get(8)?,
                tokens_used: row.get(9)?,
                retries: row.get(10)?,
                summary: row.get(11)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct RunRow {
    pub id: i64,
    pub agent: String,
    pub issue: Option<String>,
    pub trigger: String,
    pub task_preview: Option<String>,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub success: Option<i32>,
    pub status: Option<String>,
    pub tokens_used: i64,
    pub retries: i32,
    pub summary: Option<String>,
}

// ── Heartbeats ──

pub fn write_heartbeat(daemon_pid: u32, agents_count: usize, uptime_secs: u64) -> Result<()> {
    let c = conn().lock().unwrap();
    c.execute(
        "INSERT INTO heartbeats (daemon_pid, ts, agents_count, uptime_secs) VALUES (?1, ?2, ?3, ?4)",
        params![
            daemon_pid as i64,
            crate::events::now(),
            agents_count as i64,
            uptime_secs as i64,
        ],
    )?;
    // Prune old heartbeats (keep last 1000)
    c.execute(
        "DELETE FROM heartbeats WHERE id NOT IN (SELECT id FROM heartbeats ORDER BY id DESC LIMIT 1000)",
        [],
    )?;
    Ok(())
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Heartbeat {
    pub daemon_pid: i64,
    pub ts: String,
    pub agents_count: i64,
    pub uptime_secs: i64,
}

pub fn last_heartbeat() -> Result<Option<Heartbeat>> {
    let c = conn().lock().unwrap();
    let mut stmt = c.prepare(
        "SELECT daemon_pid, ts, agents_count, uptime_secs FROM heartbeats ORDER BY id DESC LIMIT 1",
    )?;
    let mut rows = stmt.query_map([], |row| {
        Ok(Heartbeat {
            daemon_pid: row.get(0)?,
            ts: row.get(1)?,
            agents_count: row.get(2)?,
            uptime_secs: row.get(3)?,
        })
    })?;
    Ok(rows.next().transpose()?)
}

// ── Stats / telemetry ──

#[derive(Debug, Clone, serde::Serialize)]
pub struct DailyStats {
    pub date: String,
    pub total_runs: i64,
    pub successful: i64,
    pub failed: i64,
    pub total_tokens: i64,
    pub agents_used: Vec<String>,
}

pub fn stats_today() -> Result<DailyStats> {
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let c = conn().lock().unwrap();

    let total_runs: i64 = c.query_row(
        "SELECT COUNT(*) FROM runs WHERE started_at LIKE ?1",
        params![format!("{today}%")],
        |r| r.get(0),
    )?;
    let successful: i64 = c.query_row(
        "SELECT COUNT(*) FROM runs WHERE started_at LIKE ?1 AND success = 1",
        params![format!("{today}%")],
        |r| r.get(0),
    )?;
    let failed: i64 = c.query_row(
        "SELECT COUNT(*) FROM runs WHERE started_at LIKE ?1 AND success = 0",
        params![format!("{today}%")],
        |r| r.get(0),
    )?;
    let total_tokens: i64 = c.query_row(
        "SELECT COALESCE(SUM(tokens_used), 0) FROM runs WHERE started_at LIKE ?1",
        params![format!("{today}%")],
        |r| r.get(0),
    )?;

    let mut stmt = c.prepare(
        "SELECT DISTINCT agent FROM runs WHERE started_at LIKE ?1",
    )?;
    let agents: Vec<String> = stmt
        .query_map(params![format!("{today}%")], |r| r.get(0))?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(DailyStats {
        date: today,
        total_runs,
        successful,
        failed,
        total_tokens,
        agents_used: agents,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_run() {
        // Use in-memory DB for test
        let c = Connection::open_in_memory().unwrap();
        migrate(&c).unwrap();

        c.execute(
            "INSERT INTO runs (agent, issue, task_preview, started_at) VALUES ('test-agent', 'foo/bar#1', 'do thing', '2026-04-12T10:00:00Z')",
            [],
        ).unwrap();
        let id = c.last_insert_rowid();

        c.execute(
            "UPDATE runs SET finished_at = '2026-04-12T10:01:00Z', success = 1, status = 'success', tokens_used = 5000, retries = 1, summary = 'done' WHERE id = ?1",
            params![id],
        ).unwrap();

        let row: (String, i64, i32) = c.query_row(
            "SELECT agent, tokens_used, success FROM runs WHERE id = ?1",
            params![id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        ).unwrap();

        assert_eq!(row.0, "test-agent");
        assert_eq!(row.1, 5000);
        assert_eq!(row.2, 1);
    }

    #[test]
    fn heartbeat_roundtrip() {
        let c = Connection::open_in_memory().unwrap();
        migrate(&c).unwrap();

        c.execute(
            "INSERT INTO heartbeats (daemon_pid, ts, agents_count, uptime_secs) VALUES (1234, '2026-04-12T10:00:00Z', 6, 3600)",
            [],
        ).unwrap();

        let (pid, count): (i64, i64) = c.query_row(
            "SELECT daemon_pid, agents_count FROM heartbeats ORDER BY id DESC LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        ).unwrap();

        assert_eq!(pid, 1234);
        assert_eq!(count, 6);
    }
}
