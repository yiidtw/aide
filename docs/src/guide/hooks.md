# Hooks

Hooks are lifecycle callbacks that run before and after a task.

## Configuration

```toml
[hooks]
on_spawn = ["inject-vault", "notify-start"]
on_complete = ["commit-memory", "notify-done"]
```

## Lifecycle

```
on_spawn hooks  →  claude -p (task loop)  →  on_complete hooks
```

1. **on_spawn** — runs before the first `claude -p` invocation
2. **task loop** — `claude -p` invocations with budget tracking
3. **on_complete** — runs after the task finishes (success or budget exhausted)

## Built-in hooks

| Hook | Phase | Description |
|------|-------|-------------|
| `inject-vault` | on_spawn | Decrypt vault and inject secrets as env vars. This is handled automatically by the runner — you rarely need to list it explicitly. |
| `commit-memory` | on_complete | `git add memory/ && git commit` in the agent directory |

## Custom hooks

Any string that isn't a built-in hook name is executed as a shell command in the agent's directory:

```toml
[hooks]
on_spawn = ["echo 'Starting task...'"]
on_complete = ["./scripts/notify-slack.sh"]
```

Custom hooks receive the agent directory as the working directory and inherit the vault env vars.
