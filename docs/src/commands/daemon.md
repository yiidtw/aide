# aide up / down

Start and stop the trigger daemon.

## aide up

```bash
aide up
```

Starts a background daemon that polls triggers for all registered agents. The daemon:

- Checks each agent's `[trigger]` setting
- For `issue` triggers: polls `gh issue list` for matching issues
- Dispatches `aide run` when a trigger fires
- Writes a PID file to `~/.aide/daemon.pid`

## aide down

```bash
aide down
```

Sends SIGTERM to the daemon process and removes the PID file.

## Notes

- Only one daemon instance runs at a time
- The daemon skips agents with `trigger.on = "manual"`
- Poll interval is configured in `~/.aide/config.toml`
