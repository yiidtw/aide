//! Minimal append-only orchestration event log.
//!
//! JSONL at `~/.aide/events.jsonl`. Written by dispatch + run-issue lifecycle.
//! Read by `aide events` for a simple timeline view.
//!
//! This is the stepping stone toward the full SQLite telemetry in #71/#77 —
//! kept deliberately small so it's usable today without schema migrations.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::io::Write;

use crate::registry;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    /// RFC3339 timestamp
    pub ts: String,
    /// Event kind: dispatched | started | finished | failed
    pub kind: String,
    /// Agent name
    pub agent: String,
    /// `owner/repo#N`
    pub issue: String,
    /// Optional status line from summary (success|partial|failed|cancelled|timeout)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// Optional tokens used (from runner result)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokens: Option<u64>,
}

fn events_path() -> std::path::PathBuf {
    registry::aide_dir().join("events.jsonl")
}

/// Append one event to the JSONL log. Best-effort — never fails the caller.
pub fn log(event: &Event) {
    if let Err(e) = append(event) {
        tracing::warn!("Failed to log event: {e:#}");
    }
}

fn append(event: &Event) -> Result<()> {
    let path = events_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .with_context(|| format!("Failed to open {}", path.display()))?;
    let line = serde_json::to_string(event)?;
    writeln!(f, "{line}")?;
    Ok(())
}

/// Read last `limit` events from the log (newest last).
pub fn recent(limit: usize) -> Result<Vec<Event>> {
    let path = events_path();
    if !path.exists() {
        return Ok(vec![]);
    }
    let content = std::fs::read_to_string(&path)?;
    let all: Vec<Event> = content
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();
    let start = all.len().saturating_sub(limit);
    Ok(all[start..].to_vec())
}

/// Current timestamp in RFC3339 format.
pub fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// Pretty-print a sequence of events as a timeline.
pub fn print_timeline(events: &[Event]) {
    if events.is_empty() {
        println!("(no events yet — dispatch an agent to populate the log)");
        return;
    }
    println!(
        "{:<25} {:<12} {:<20} {:<30} {:<10} {}",
        "TIME", "KIND", "AGENT", "ISSUE", "STATUS", "TOKENS"
    );
    println!("{}", "─".repeat(110));
    for e in events {
        // Trim ts to "YYYY-MM-DD HH:MM:SS"
        let short_ts = e
            .ts
            .get(..19)
            .map(|s| s.replace('T', " "))
            .unwrap_or_else(|| e.ts.clone());
        let status = e.status.clone().unwrap_or_else(|| "-".into());
        let tokens = e
            .tokens
            .map(|t| t.to_string())
            .unwrap_or_else(|| "-".into());
        println!(
            "{:<25} {:<12} {:<20} {:<30} {:<10} {}",
            short_ts, e.kind, e.agent, e.issue, status, tokens
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_roundtrip() {
        let e = Event {
            ts: "2026-04-11T10:00:00Z".into(),
            kind: "dispatched".into(),
            agent: "crossmem-rs".into(),
            issue: "crossmem/crossmem-rs#1".into(),
            status: None,
            tokens: None,
        };
        let s = serde_json::to_string(&e).unwrap();
        let back: Event = serde_json::from_str(&s).unwrap();
        assert_eq!(back.kind, "dispatched");
        assert_eq!(back.agent, "crossmem-rs");
    }

    #[test]
    fn finished_event_with_status_and_tokens() {
        let e = Event {
            ts: "2026-04-11T10:05:00Z".into(),
            kind: "finished".into(),
            agent: "crossmem-rs".into(),
            issue: "crossmem/crossmem-rs#1".into(),
            status: Some("success".into()),
            tokens: Some(42_000),
        };
        let s = serde_json::to_string(&e).unwrap();
        assert!(s.contains("\"status\":\"success\""));
        assert!(s.contains("\"tokens\":42000"));
    }
}
