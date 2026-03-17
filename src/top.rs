use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{prelude::*, widgets::*};
use std::collections::HashMap;
use std::io;
use std::time::Duration;

use crate::agents::instance::InstanceManager;

/// Run the live TUI dashboard (`aide.sh top`).
pub fn run_top(data_dir: &str) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    crossterm::execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mgr = InstanceManager::new(data_dir);
    let result = run_loop(&mut terminal, &mgr);

    // Restore terminal regardless of result
    disable_raw_mode()?;
    crossterm::execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

fn run_loop(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, mgr: &InstanceManager) -> Result<()> {
    let mut selected: usize = 0;

    loop {
        let instances = mgr.list().unwrap_or_default();
        let instance_count = instances.len();

        // Gather skill stats for the selected instance
        let skill_stats = if !instances.is_empty() {
            let sel = selected.min(instance_count.saturating_sub(1));
            parse_skill_stats(mgr, &instances[sel].name)
        } else {
            Vec::new()
        };

        let sel_name = if !instances.is_empty() {
            let sel = selected.min(instance_count.saturating_sub(1));
            instances[sel].name.clone()
        } else {
            String::new()
        };

        terminal.draw(|f| ui(f, &instances, selected, &sel_name, &skill_stats))?;

        // Poll for input — refresh every 2 seconds
        if event::poll(Duration::from_secs(2))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    KeyCode::Down | KeyCode::Char('j') => {
                        if instance_count > 0 {
                            selected = (selected + 1) % instance_count;
                        }
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        if instance_count > 0 {
                            selected = (selected + instance_count - 1) % instance_count;
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    Ok(())
}

/// Skill usage entry for display.
struct SkillStat {
    name: String,
    count: u64,
    success: u64,
    #[allow(dead_code)]
    fail: u64,
}

/// Parse logs for an instance and return skill usage stats, sorted by count descending.
fn parse_skill_stats(mgr: &InstanceManager, name: &str) -> Vec<SkillStat> {
    let logs = match mgr.read_logs(name, 10000) {
        Ok(l) => l,
        Err(_) => return Vec::new(),
    };

    let mut by_skill: HashMap<String, (u64, u64, u64)> = HashMap::new();

    for line in &logs {
        let rest = if let Some(pos) = line.find("mcp-exec-result: ") {
            &line[pos + "mcp-exec-result: ".len()..]
        } else if let Some(pos) = line.find("exec-result: ") {
            &line[pos + "exec-result: ".len()..]
        } else {
            continue;
        };

        // Parse: <skill> ... -> ok/FAILED
        let arrow_pos = rest.find(" \u{2192} ")
            .or_else(|| rest.find(" -> "));
        let arrow_pos = match arrow_pos {
            Some(p) => p,
            None => continue,
        };

        let skill_full = &rest[..arrow_pos];
        let skill_name = skill_full.split_whitespace().next().unwrap_or(skill_full);

        let after_arrow = if rest[arrow_pos..].starts_with(" \u{2192} ") {
            &rest[arrow_pos + " \u{2192} ".len()..]
        } else {
            &rest[arrow_pos + " -> ".len()..]
        };
        let is_success = after_arrow.starts_with("ok");

        let entry = by_skill.entry(skill_name.to_string()).or_insert((0, 0, 0));
        entry.0 += 1;
        if is_success {
            entry.1 += 1;
        } else {
            entry.2 += 1;
        }
    }

    let mut stats: Vec<SkillStat> = by_skill
        .into_iter()
        .map(|(name, (count, success, fail))| SkillStat { name, count, success, fail })
        .collect();
    stats.sort_by(|a, b| b.count.cmp(&a.count));
    stats
}

fn ui(
    f: &mut Frame,
    instances: &[crate::agents::instance::InstanceInfo],
    selected: usize,
    sel_name: &str,
    skill_stats: &[SkillStat],
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),  // title
            Constraint::Min(5),    // instances table
            Constraint::Length(skill_stats.len().min(12) as u16 + 3), // usage block
        ])
        .split(f.area());

    // Title bar
    let count = instances.len();
    let title = Paragraph::new(Line::from(vec![
        Span::styled(
            format!("aide.sh top \u{2014} {} instance{}", count, if count == 1 { "" } else { "s" }),
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ),
        Span::raw("    "),
        Span::styled("[q] quit  [\u{2191}\u{2193}] select", Style::default().fg(Color::DarkGray)),
    ]));
    f.render_widget(title, chunks[0]);

    // Instance table
    let header = Row::new(vec!["INSTANCE", "TYPE", "CRON", "STATUS", "LAST ACTIVITY"])
        .style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
        .bottom_margin(1);

    let rows: Vec<Row> = instances
        .iter()
        .enumerate()
        .map(|(i, inst)| {
            let status_style = match inst.status {
                crate::agents::instance::InstanceStatus::Active => Style::default().fg(Color::Green),
                crate::agents::instance::InstanceStatus::Stopped => Style::default().fg(Color::Red),
            };
            let last = inst.last_activity.as_deref().unwrap_or("\u{2014}");
            // Truncate last activity to keep table tidy
            let last_trunc = if last.len() > 50 { &last[..50] } else { last };

            let row = Row::new(vec![
                Cell::from(inst.name.clone()),
                Cell::from(inst.agent_type.clone()),
                Cell::from(inst.cron_count.to_string()),
                Cell::from(inst.status.to_string()).style(status_style),
                Cell::from(last_trunc.to_string()),
            ]);
            if i == selected {
                row.style(Style::default().bg(Color::DarkGray))
            } else {
                row
            }
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(20),
            Constraint::Length(12),
            Constraint::Length(6),
            Constraint::Length(10),
            Constraint::Min(20),
        ],
    )
    .header(header)
    .block(Block::default().borders(Borders::ALL).title(" Instances "));

    f.render_widget(table, chunks[1]);

    // Skill usage bars
    if skill_stats.is_empty() {
        let empty = Paragraph::new("No skill usage data")
            .style(Style::default().fg(Color::DarkGray))
            .block(Block::default().borders(Borders::ALL).title(" Skill Usage "));
        f.render_widget(empty, chunks[2]);
        return;
    }

    let max_count = skill_stats.iter().map(|s| s.count).max().unwrap_or(1);
    let bar_max_width: u64 = 20;

    let rows: Vec<Row> = skill_stats
        .iter()
        .map(|s| {
            let bar_len = ((s.count as f64 / max_count as f64) * bar_max_width as f64) as usize;
            let bar = "\u{2588}".repeat(bar_len);
            let pad = " ".repeat(bar_max_width as usize - bar_len);
            let pct = if s.count > 0 {
                (s.success as f64 / s.count as f64 * 100.0) as u64
            } else {
                0
            };
            Row::new(vec![
                Cell::from(s.name.clone()),
                Cell::from(format!("{}{}", bar, pad)).style(Style::default().fg(Color::Green)),
                Cell::from(format!("{}/{}", s.success, s.count)),
                Cell::from(format!("{}%", pct)),
            ])
        })
        .collect();

    let usage_table = Table::new(
        rows,
        [
            Constraint::Length(14),
            Constraint::Length(22),
            Constraint::Length(8),
            Constraint::Length(6),
        ],
    )
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(format!(" Skill Usage ({}) ", sel_name)),
    );

    f.render_widget(usage_table, chunks[2]);
}
