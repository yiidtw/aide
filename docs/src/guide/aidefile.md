# Aidefile

The Aidefile is a TOML config file that turns a Claude Code project into an agent. Place it in the project root.

## Full example

```toml
[persona]
name = "Senior Reviewer"
style = "direct, cares about edge cases"

[budget]
tokens = "100k"
max_retries = 3

[memory]
compact_after = "200k"

[hooks]
on_spawn = ["inject-vault"]
on_complete = ["commit-memory"]

[skills]
include = ["code-review"]

[trigger]
on = "issue"

[vault]
keys = ["GITHUB_TOKEN", "SLACK_WEBHOOK"]
```

## Sections

### `[persona]`

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `name` | string | `"unnamed"` | Agent display name |
| `style` | string | — | Personality hint (injected into CLAUDE.md context) |

### `[budget]`

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `tokens` | string | `"200k"` | Token limit per task. Supports `"100k"`, `"1m"`, or raw numbers. |
| `max_retries` | integer | `3` | Maximum retry attempts if task doesn't complete in one invocation |

### `[memory]`

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `compact_after` | string | — | Auto-compact memory when estimated tokens exceed this threshold |

### `[hooks]`

| Field | Type | Description |
|-------|------|-------------|
| `on_spawn` | list of strings | Run before the task starts |
| `on_complete` | list of strings | Run after the task finishes |

Built-in hooks:
- `"inject-vault"` — decrypt and inject vault secrets (handled automatically by the runner)
- `"commit-memory"` — git add + commit the `memory/` directory

Custom hooks are shell commands run in the agent's directory.

### `[skills]`

| Field | Type | Description |
|-------|------|-------------|
| `include` | list of strings | Skill names to load from the `skills/` directory |

### `[trigger]`

| Field | Type | Description |
|-------|------|-------------|
| `on` | string | Trigger type: `"manual"`, `"issue"`, or `"cron:EXPR"` |

### `[vault]`

| Field | Type | Description |
|-------|------|-------------|
| `keys` | list of strings | Secret names to decrypt and inject as env vars |

## Token shorthand

The `tokens` and `compact_after` fields accept shorthand:

| Input | Value |
|-------|-------|
| `"100k"` | 100,000 |
| `"1m"` | 1,000,000 |
| `"50000"` | 50,000 |

## Minimal Aidefile

Only `[persona]` is required:

```toml
[persona]
name = "Helper"
```

Everything else has sensible defaults (200k token budget, manual trigger, no vault).
