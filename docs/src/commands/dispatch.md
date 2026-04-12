# aide dispatch / wait / events

Commands for inter-agent delegation and observability.

## aide dispatch

```bash
aide dispatch <agent> "<task>"
aide dispatch reviewer "Review PR #42 for security issues"
aide dispatch --dry-run deployer "Ship v1.2.0"
```

Creates a GitHub issue labeled for the target agent, spawns a detached `aide run-issue` background worker, and returns immediately with the issue reference and a wait command you can use to block on the result.

### Flags

| Flag | Description |
|------|-------------|
| `--dry-run` | Print the issue that would be created without actually creating it or spawning a worker |

### Output

```
Dispatched: https://github.com/org/repo/issues/99
Wait with: aide wait https://github.com/org/repo/issues/99
```

## aide wait

```bash
aide wait <issue-url>
aide wait --timeout 30m https://github.com/org/repo/issues/99
```

Blocks until the target issue closes, then prints the bounded summary extracted from the closing comment.

### Flags

| Flag | Default | Description |
|------|---------|-------------|
| `--timeout` | `60m` | Maximum time to wait before returning exit code 124 |
| `--poll-interval` | `10s` | How often to check issue status |

### Exit codes

| Code | Meaning |
|------|---------|
| 0 | Sub-agent completed successfully |
| 1 | Sub-agent reported partial completion or failure |
| 2 | Issue was cancelled (closed without summary) |
| 124 | Timeout reached |

### Output

On success, prints the content of the `<aide-summary>` block from the issue's closing comment. This is the bounded summary controlled by the sub-agent's `[output]` config.

## aide events

```bash
aide events
aide events --limit 20
```

Reads `~/.aide/events.jsonl` and prints a timeline table of recent dispatch and completion events.

### Flags

| Flag | Default | Description |
|------|---------|-------------|
| `--limit` | `50` | Number of events to show |

### Example output

```
TIME            EVENT       AGENT       ISSUE   STATUS
2026-04-11 14:01 dispatch   reviewer    #99     running
2026-04-11 14:03 complete   reviewer    #99     success
2026-04-11 14:05 dispatch   deployer    #100    running
```

## aide api

```bash
aide api
aide api --port 7979
```

Starts a local HTTP API server for programmatic access to aide state.

### Flags

| Flag | Default | Description |
|------|---------|-------------|
| `--port` | `7979` | Port to listen on |

### Endpoints

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/runs` | GET | List recent runs with status and token usage |
| `/api/agents` | GET | List registered agents |
| `/api/heartbeat` | GET | Last heartbeat timestamp from daemon |
| `/api/stats` | GET | Aggregate stats (today's runs, tokens, agents) |
| `/api/health` | GET | Health check (returns 200 if daemon is running) |

## aide stats

```bash
aide stats
```

Prints a summary of today's activity from the local SQLite database:

```
Today (2026-04-11):
  Runs:   12
  Tokens: 847,230
  Agents: 3 (reviewer, deployer, helper)
```

## aide status

```bash
aide status
```

Shows daemon health and last heartbeat:

```
Daemon: running (pid 12345)
Last heartbeat: 2s ago
Uptime: 4h 12m
```

## aide install-service / aide uninstall-service

```bash
aide install-service
aide uninstall-service
```

Installs or removes the aide daemon as a system service so it starts automatically on login.

### Platform support

| Platform | Mechanism | Location |
|----------|-----------|----------|
| macOS | launchd plist | `~/Library/LaunchAgents/sh.aide.daemon.plist` |
| Linux | systemd user unit | `~/.config/systemd/user/aide-daemon.service` |

After installing, the daemon starts on login and restarts on failure. Use `aide uninstall-service` to remove the service definition and stop the daemon.
