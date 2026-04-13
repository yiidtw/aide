# aide dashboard

Live terminal UI for monitoring dispatches in flight.

```bash
aide dashboard
```

Built on [ratatui](https://ratatui.rs) — pure Rust, no external dependencies.

## Layout

| Panel | Content |
|-------|---------|
| **Top bar** | Title, timestamp, active dispatch count |
| **Left** | Active dispatches table (agent, issue, elapsed, tokens) |
| **Right** | Registered agents (name, trigger, status) |
| **Bottom** | Recent events timeline (scrollable) |

## Key bindings

| Key | Action |
|-----|--------|
| `q` / `Esc` | Quit |
| `↑` / `↓` | Scroll events |
| `r` | Force refresh |

Auto-refreshes every 2 seconds.

## Data sources

- Active dispatches: derived from `~/.aide/events.jsonl` (dispatched/started without matching finished/failed)
- Agents: loaded from the aide registry
- Events: last 100 from `~/.aide/events.jsonl`
- Runs: from `~/.aide/state.db`

## See also

- [`aide-skill aide watch`](../guide/skills.md) — lightweight terminal monitor (no TUI, just prints)
- [`aide-skill aide serve`](../guide/skills.md) — web dashboard at localhost:7610
