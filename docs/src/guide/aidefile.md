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

[output]
max_summary_tokens = 500
narrative_schema = """
NOTES: <one-line what you changed>
PR: <url or none>
NEXT: <optional redirect>
"""

[workspace]
read = ["~/claude_projects/crossmem-bridge", "~/claude_projects/crossmem-chrome"]
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

### `[output]`

```toml
[output]
max_summary_tokens = 500
narrative_schema = """
NOTES: <one-line what you changed>
PR: <url or none>
NEXT: <optional redirect>
"""
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `max_summary_tokens` | u32 | 500 | Max tokens in the bounded summary |
| `narrative_schema` | string | (default NOTES/PR/NEXT) | Template the sub-agent fills in inside `<aide-summary>` block |

This section is load-bearing for aide-as-subagent mode. Without it, sub-agent output pollutes the frontier context. The runner wraps the task with instructions requiring the sub-agent to emit an `<aide-summary>` block conforming to the `narrative_schema`. The bounded summary is what `aide wait` returns to the calling agent, keeping the frontier context clean.

### `[workspace]`

```toml
[workspace]
read = ["~/claude_projects/crossmem-bridge", "~/claude_projects/crossmem-chrome"]
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `read` | list of strings | [] | Sibling directories the sub-agent can read |

The `read` list is translated to `.claude/settings.json` permission grants and a WORKSPACE section in the task prompt. This solves sandbox restrictions in non-interactive `claude -p` mode, where the sub-agent would otherwise be unable to access files outside its own project directory.

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
