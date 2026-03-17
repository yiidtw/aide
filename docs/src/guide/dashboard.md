# Dashboard

aide.sh includes a built-in web dashboard for monitoring agents.

## Standalone mode

```bash
$ aide.sh dash
Dashboard running at http://localhost:3939
```

Opens a web UI showing all running instances, recent logs, and skill execution history. Press `Ctrl+C` to stop.

## Daemon mode

When running the daemon, the dashboard is included by default:

```bash
$ aide.sh up
Daemon started (PID 12345)
Dashboard: http://localhost:3939
Cron scheduler: active
```

To run the daemon without the dashboard:

```bash
$ aide.sh up --no-dash
Daemon started (PID 12345)
Cron scheduler: active
```

## Dashboard UI

The dashboard displays:

- **Instances panel** — list of running agents with status (running / stopped / error)
- **Logs panel** — real-time log stream for all instances, filterable by instance name
- **Skills panel** — per-instance skill list with last execution time, exit code, and duration
- **Cron panel** — scheduled skills with next run time and execution history
- **Vault panel** — secret names and last-set dates (values are never shown)

## API endpoints

The dashboard exposes a REST API on the same port:

```bash
# List instances
$ curl http://localhost:3939/api/instances
[{"name": "jenny", "image": "jenny:0.1.0", "status": "running"}]

# Get logs
$ curl http://localhost:3939/api/instances/jenny/logs?limit=50

# Execute a skill
$ curl -X POST http://localhost:3939/api/instances/jenny/exec \
  -H "Content-Type: application/json" \
  -d '{"skill": "cool", "args": ["courses"]}'

# Cron status
$ curl http://localhost:3939/api/cron
```

## Stopping the daemon

```bash
$ aide.sh down
Daemon stopped.
```
