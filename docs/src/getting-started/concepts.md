# Concepts

## What is an agent?

An agent is a Claude Code project with an Aidefile. That's it.

The Aidefile declares persona, budget, vault, hooks, and triggers. aide handles the lifecycle — Claude Code handles the thinking.

## aide vs Claude Code

It's important to understand what's native to Claude Code and what aide adds:

### Claude Code native (works without aide)

| Feature | How it works |
|---------|-------------|
| `claude -p "task"` | Headless Claude Code — runs a task and exits |
| `.claude/agents/*.md` | Custom subagent definitions, dispatched via Agent tool |
| `~/.claude/projects/*/memory/` | Auto-memory, Claude Code manages per-project |
| `CLAUDE.md` | Project instructions, read on session start |
| Hooks | PreToolUse, PostToolUse, PreCompact lifecycle events |

A single Claude Code project doesn't need aide. Claude Code is already a capable agent runtime.

### aide adds

| Feature | How it works |
|---------|-------------|
| Aidefile | Single config: persona, budget, vault, hooks, trigger, skills |
| Token isolation | `aide dispatch` runs work in separate `claude -p` processes |
| Vault | Encrypted secrets, injected as env vars at spawn time |
| Team memory | Centralized at HQ, agents are stateless |
| Policy routing | Deterministic rules decide which agent gets which task |
| Skill injection | Policy controls which skills are injected per task |
| Telemetry | Token usage, duration, success/fail per dispatch |
| Daemon | Background polling for trigger-based automation |

## Aidefile

The single config file that turns a project into an agent:

```toml
[persona]
name = "Senior Reviewer"
style = "direct, cares about edge cases"

[budget]
tokens = "100k"
max_retries = 3

[trigger]
on = "issue"

[vault]
keys = ["GITHUB_TOKEN"]

[skills]
include = ["code-review"]
```

The Aidefile is safe to commit to public repos — it contains no secrets, no memory, no state.

## Two layers

### Layer 1: Single agent

```
any-project/
├── Aidefile        ← this is all you need
├── src/
└── ...
```

`aide run . "task"` — budget control, vault injection, that's it. No HQ, no orchestration.

### Layer 2: Team (HQ)

```
crossmem-hq/                 ← coordinator (private)
├── CLAUDE.md
├── memory/
│   ├── _shared/             ← team-level context
│   ├── crossmem-rs/         ← per-agent memory
│   └── crossmem-web/
├── policy.toml              ← routing rules
├── vault.toml               ← secrets
└── .claude/agents/          ← auto-generated wrappers

crossmem-rs/                  ← member (can be public)
├── Aidefile
└── src/

crossmem-web/                 ← member (can be public)
├── Aidefile
└── src/
```

HQ is the single source of truth for memory, policy, vault, and telemetry. Member agents are stateless — they receive context at spawn time and return output. They don't store anything locally.

## Registry

aide keeps a registry at `~/.aide/config.toml` mapping agent names to directories:

```
reviewer  →  ~/projects/code-reviewer
writer    →  ~/projects/blog-writer
ops       →  ~/.aide/ops
```

- **`aide spawn <name>`** — creates a new directory under `~/.aide/<name>/` with a template Aidefile
- **`aide register <path>`** — registers an existing project that already has an Aidefile

## Dispatch flow

```
aide dispatch crossmem-rs "fix parser bug"
  ├─ Policy check: route match? deny match?
  ├─ Vault injection: GITHUB_TOKEN → env var
  ├─ Memory injection: HQ/memory/ → prompt context
  ├─ Skill injection: policy decides which skills
  ├─ claude -p "fix parser bug"    ← isolated process
  ├─ Output → HQ decides what to write back to memory
  └─ Telemetry: tokens, duration, success/fail
```

The frontier Claude Code session only sees the bounded summary. 50k tokens of agent work compresses to ~500 tokens of output.

## Daemon

`aide up` starts a background polling loop that checks triggers:

- **issue** — polls `gh issue list` for matching issues
- **cron** — runs on a schedule (coming soon)
- **manual** — no auto-trigger, only responds to `aide run` / `aide dispatch`

When a trigger fires, the daemon calls `aide run` for that agent.
