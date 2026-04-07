# Triggers & Daemon

Triggers define what wakes an agent up. The daemon (`aide up`) polls triggers and dispatches tasks.

## Trigger types

### manual (default)

```toml
[trigger]
on = "manual"
```

Agent only runs when you explicitly call `aide run`.

### issue

```toml
[trigger]
on = "issue"
```

The daemon polls `gh issue list --label <agent_name>` in the agent's git repo. When a matching issue is found:

1. The issue title + body become the task
2. aide runs the agent with that task
3. On completion, aide comments on the issue and closes it

### cron (planned)

```toml
[trigger]
on = "cron:0 9 * * *"
```

Run the agent on a schedule. Not yet implemented.

## Daemon

### Start

```bash
aide up
```

Starts a background polling loop. The daemon:
- Reads the registry to find all agents with non-manual triggers
- Polls each agent's trigger on the configured interval
- Dispatches `aide run` when a trigger fires
- Writes a PID file for lifecycle management

### Stop

```bash
aide down
```

Sends SIGTERM to the daemon process and cleans up the PID file.

## GitHub repo detection

For issue triggers, aide detects the GitHub repo by parsing the git remote URL in the agent's directory. Both HTTPS and SSH formats are supported:

- `https://github.com/user/repo.git`
- `git@github.com:user/repo.git`
