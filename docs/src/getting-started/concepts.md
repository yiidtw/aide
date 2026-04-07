# Concepts

## What is an agent?

An agent is a Claude Code project that can autonomously execute tasks. The difference between a regular project and an agent is:

1. **Aidefile** — a TOML config that defines persona, budget, vault, hooks, and triggers
2. **aide binary** — the lifecycle manager that spawns `claude -p`, enforces budget, injects secrets, and handles triggers

An agent is not a long-running process. It's a directory with an Aidefile. aide wakes it up on demand, gives it a task, and collects the result.

## Aidefile

The single config file that turns a Claude project into an agent. Everything aide needs to know is in the Aidefile:

- **persona** — who the agent is (name, style)
- **budget** — token limit and retry policy
- **vault** — which secrets to inject
- **hooks** — lifecycle callbacks (on_spawn, on_complete)
- **trigger** — what wakes the agent up (manual, issue, cron)
- **memory** — auto-compaction threshold
- **skills** — which skill sets to include

See the [Aidefile reference](../guide/aidefile.md) for all options.

## Registry

aide keeps a registry at `~/.aide/config.toml` mapping agent names to directories:

```
reviewer  →  ~/projects/code-reviewer
writer    →  ~/projects/blog-writer
ops       →  ~/.aide/ops
```

There are two ways to create agents:

- **`aide spawn <name>`** — creates a new directory under `~/.aide/<name>/` with a template Aidefile
- **`aide register <path>`** — registers an existing Claude project that already has an Aidefile

Either way, once registered, you can `aide run <name> "task"`.

## How aide run works

```
aide run reviewer "Review PR #42"
       │
       ├─ resolve "reviewer" → ~/projects/code-reviewer
       ├─ load Aidefile
       ├─ decrypt vault → env vars
       ├─ run on_spawn hooks
       ├─ loop: claude -p "Review PR #42"
       │    └─ check budget after each invocation
       ├─ run on_complete hooks
       └─ check memory compaction threshold
```

aide is the lifecycle manager. Claude Code is the runtime. All intelligence is in Claude Code — aide just handles who to wake up, how much to spend, what secrets to give, and when to stop.

## Daemon

`aide up` starts a background polling loop that checks triggers:

- **issue** — polls `gh issue list` for issues with the agent's label
- **cron** — runs on a schedule (coming soon)
- **manual** — no auto-trigger, only responds to `aide run`

When a trigger fires, the daemon calls `aide run` for that agent.
