//! Live TUI dashboard for aide orchestration.
//!
//! Displays active dispatches, registered agents, and recent events
//! in a ratatui terminal UI with auto-refresh.

use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode},
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use ratatui::{
    prelude::*,
    widgets::*,
};
use std::io;
use std::time::Duration;

use crate::{aidefile, db, events, registry};

struct AppState {
    events: Vec<events::Event>,
    runs: Vec<db::RunRow>,
    agents: Vec<registry::AgentEntry>,
    event_scroll: usize,
    last_refresh: String,
}

impl AppState {
    fn refresh(&mut self) {
        self.events = events::recent(100).unwrap_or_default();
        self.runs = db::recent_runs(50).unwrap_or_default();
        self.agents = registry::list().unwrap_or_default();
        self.last_refresh = chrono::Local::now().format("%H:%M:%S").to_string();
    }

    fn active_count(&self) -> usize {
        // Events that have been dispatched/started but not finished/failed
        let mut active = std::collections::HashSet::new();
        for e in &self.events {
            match e.kind.as_str() {
                "dispatched" | "started" => {
                    active.insert(e.issue.clone());
                }
                "finished" | "failed" => {
                    active.remove(&e.issue);
                }
                _ => {}
            }
        }
        active.len()
    }

    fn active_dispatches(&self) -> Vec<ActiveDispatch> {
        let mut map: std::collections::HashMap<String, ActiveDispatch> =
            std::collections::HashMap::new();
        for e in &self.events {
            match e.kind.as_str() {
                "dispatched" | "started" => {
                    map.entry(e.issue.clone())
                        .or_insert_with(|| ActiveDispatch {
                            agent: e.agent.clone(),
                            issue: e.issue.clone(),
                            started: e.ts.clone(),
                            tokens: e.tokens.unwrap_or(0),
                        });
                }
                "finished" | "failed" => {
                    map.remove(&e.issue);
                }
                _ => {}
            }
        }
        let mut result: Vec<_> = map.into_values().collect();
        result.sort_by(|a, b| a.started.cmp(&b.started));
        result
    }

    fn max_event_scroll(&self) -> usize {
        self.events.len().saturating_sub(1)
    }
}

struct ActiveDispatch {
    agent: String,
    issue: String,
    started: String,
    tokens: u64,
}

pub fn run_dashboard() -> Result<()> {
    // Setup terminal
    terminal::enable_raw_mode()?;
    io::stdout().execute(EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;

    let mut state = AppState {
        events: vec![],
        runs: vec![],
        agents: vec![],
        event_scroll: 0,
        last_refresh: String::new(),
    };
    state.refresh();

    loop {
        terminal.draw(|frame| ui(frame, &state))?;

        if event::poll(Duration::from_secs(2))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    KeyCode::Char('r') => state.refresh(),
                    KeyCode::Down => {
                        if state.event_scroll < state.max_event_scroll() {
                            state.event_scroll += 1;
                        }
                    }
                    KeyCode::Up => {
                        state.event_scroll = state.event_scroll.saturating_sub(1);
                    }
                    _ => {}
                }
            }
        } else {
            // Timeout — auto-refresh
            state.refresh();
        }
    }

    // Restore terminal
    terminal::disable_raw_mode()?;
    io::stdout().execute(LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

fn ui(frame: &mut Frame, state: &AppState) {
    let area = frame.area();

    // 3-row layout: top bar, middle panels, bottom events
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // top bar
            Constraint::Min(8),    // middle panels
            Constraint::Length(12), // bottom events
        ])
        .split(area);

    render_top_bar(frame, layout[0], state);
    render_middle(frame, layout[1], state);
    render_events(frame, layout[2], state);
}

fn render_top_bar(frame: &mut Frame, area: Rect, state: &AppState) {
    let active = state.active_count();
    let text = vec![
        Span::styled("aide dashboard", Style::default().fg(Color::Cyan).bold()),
        Span::raw("  |  "),
        Span::raw(format!("refreshed: {}", state.last_refresh)),
        Span::raw("  |  "),
        Span::styled(
            format!("active: {active}"),
            Style::default().fg(if active > 0 {
                Color::Yellow
            } else {
                Color::Green
            }),
        ),
        Span::raw("  |  "),
        Span::styled(
            "q:quit  r:refresh  \u{2191}\u{2193}:scroll",
            Style::default().fg(Color::DarkGray),
        ),
    ];
    let paragraph = Paragraph::new(Line::from(text))
        .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::DarkGray)));
    frame.render_widget(paragraph, area);
}

fn render_middle(frame: &mut Frame, area: Rect, state: &AppState) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(area);

    render_dispatches(frame, chunks[0], state);
    render_agents(frame, chunks[1], state);
}

fn render_dispatches(frame: &mut Frame, area: Rect, state: &AppState) {
    let active = state.active_dispatches();

    let header = Row::new(vec!["AGENT", "ISSUE", "ELAPSED", "TOKENS"])
        .style(Style::default().fg(Color::Cyan).bold());

    let now = chrono::Utc::now();
    let rows: Vec<Row> = active
        .iter()
        .map(|d| {
            let elapsed = chrono::DateTime::parse_from_rfc3339(&d.started)
                .ok()
                .map(|ts| {
                    let secs = (now - ts.with_timezone(&chrono::Utc)).num_seconds();
                    if secs < 60 {
                        format!("{secs}s")
                    } else {
                        format!("{}m{}s", secs / 60, secs % 60)
                    }
                })
                .unwrap_or_else(|| "?".into());
            let tokens = if d.tokens > 0 {
                format!("{}k", d.tokens / 1000)
            } else {
                "-".into()
            };
            Row::new(vec![
                d.agent.clone(),
                d.issue.clone(),
                elapsed,
                tokens,
            ])
            .style(Style::default().fg(Color::Yellow))
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Percentage(25),
            Constraint::Percentage(40),
            Constraint::Percentage(15),
            Constraint::Percentage(20),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .title(" Active Dispatches ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow)),
    );

    frame.render_widget(table, area);
}

fn render_agents(frame: &mut Frame, area: Rect, state: &AppState) {
    let header = Row::new(vec!["NAME", "TRIGGER", "STATUS"])
        .style(Style::default().fg(Color::Cyan).bold());

    let rows: Vec<Row> = state
        .agents
        .iter()
        .map(|a| {
            let path = std::path::PathBuf::from(shellexpand::tilde(&a.path).as_ref());
            let (trigger, status) = if aidefile::exists(&path) {
                match aidefile::load(&path) {
                    Ok(af) => (af.trigger.on.clone(), "ok"),
                    Err(_) => ("?".into(), "error"),
                }
            } else {
                ("-".into(), "missing")
            };
            let status_style = match status {
                "ok" => Style::default().fg(Color::Green),
                "error" => Style::default().fg(Color::Red),
                _ => Style::default().fg(Color::DarkGray),
            };
            Row::new(vec![
                Cell::from(a.name.clone()),
                Cell::from(trigger),
                Cell::from(Span::styled(status, status_style)),
            ])
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Percentage(40),
            Constraint::Percentage(30),
            Constraint::Percentage(30),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .title(" Agents ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan)),
    );

    frame.render_widget(table, area);
}

fn render_events(frame: &mut Frame, area: Rect, state: &AppState) {
    let header = Row::new(vec!["TIME", "KIND", "AGENT", "ISSUE", "STATUS", "TOKENS"])
        .style(Style::default().fg(Color::Cyan).bold());

    let rows: Vec<Row> = state
        .events
        .iter()
        .rev() // newest first
        .skip(state.event_scroll)
        .map(|e| {
            let short_ts = e
                .ts
                .get(..19)
                .map(|s| s.replace('T', " "))
                .unwrap_or_else(|| e.ts.clone());
            let status = e.status.clone().unwrap_or_else(|| "-".into());
            let tokens = e
                .tokens
                .map(|t| format!("{}k", t / 1000))
                .unwrap_or_else(|| "-".into());

            let style = match e.kind.as_str() {
                "dispatched" | "started" => Style::default().fg(Color::Yellow),
                "finished" => {
                    if status == "success" {
                        Style::default().fg(Color::Green)
                    } else {
                        Style::default().fg(Color::Yellow)
                    }
                }
                "failed" => Style::default().fg(Color::Red),
                _ => Style::default(),
            };

            Row::new(vec![
                short_ts,
                e.kind.clone(),
                e.agent.clone(),
                e.issue.clone(),
                status,
                tokens,
            ])
            .style(style)
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(20),
            Constraint::Length(12),
            Constraint::Length(20),
            Constraint::Min(25),
            Constraint::Length(10),
            Constraint::Length(8),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .title(format!(
                " Events ({}/{}) ",
                state.event_scroll + 1,
                state.events.len().max(1)
            ))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray)),
    );

    frame.render_widget(table, area);
}
