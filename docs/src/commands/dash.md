# aide.sh dash

Open the agent observability dashboard.

## Usage

```
aide.sh dash [-p PORT]
```

## Options

| Flag | Description |
|------|-------------|
| `-p, --port PORT` | Port to serve on (default: `3939`) |

## Description

Starts a local HTTP server serving a web UI for monitoring agent instances. The dashboard provides a read-only view of instance status, skills, cron schedules, and logs.

Static assets are embedded in the binary via `rust_embed`.

## API endpoints

| Method | Path | Description |
|--------|------|-------------|
| GET | `/` | Dashboard web UI (index.html) |
| GET | `/api/instances` | List all instances with status, type, email, role, cron count, last activity |
| GET | `/api/instance/{name}` | Instance detail: metadata, version, description, author, skills, cron entries |
| GET | `/api/logs/{name}?tail=N` | Recent log lines for an instance (default tail: 100) |

### Response examples

**GET /api/instances**
```json
{
  "instances": [
    {
      "name": "jenny.ydwu",
      "agent_type": "jenny",
      "status": "active",
      "email": "jenny@aide.sh",
      "role": "PhD assistant",
      "cron_count": 2,
      "last_activity": "[08:00:01] cool scan completed"
    }
  ]
}
```

**GET /api/logs/jenny.ydwu?tail=5**
```json
{
  "logs": [
    "[08:00:01] cron: cool scan",
    "[08:00:03] cool scan completed",
    "[12:00:01] cron: email check",
    "[12:00:05] email check completed",
    "[14:32:10] mcp-exec: cool courses"
  ]
}
```

## Integration with aide.sh up

When running `aide.sh up`, the dashboard is spawned as a background task within the daemon unless `--no-dash` is passed.

```bash
aide.sh up              # starts daemon + dashboard on port 3939
aide.sh up --no-dash    # starts daemon without dashboard
```
