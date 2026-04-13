# aide.sh

**Your commander of agents.**

aide turns any Claude Code project into an agent with one file. Multiple agents? One HQ to command them all.

## Why aide?

**The token problem:** A frontier Claude Code session has a finite context window. Every subtask handled inline eats tokens. With 5 agents doing 50k tokens each, you burn 250k of context — most of it irrelevant to the next task.

**aide's answer:** Process isolation. `aide dispatch` runs work in separate `claude -p` processes. The frontier only sees a bounded summary. 50k tokens of work → ~500 tokens of output.

## Quick taste

```bash
# Install
cargo install aide-sh

# Turn any project into an agent
cd ~/projects/code-reviewer
aide init
# ✓ Created Aidefile

# Run a task
aide run . "Review PR #42 and leave comments"
# ✓ Task completed (23,847 tokens used)

# Team mode: coordinate multiple agents
aide init --team
# ✓ Created crossmem-hq/

aide dispatch crossmem-rs "fix parser bug"
aide dispatch crossmem-web "update dashboard"
aide wait crossmem-rs#42
# ✓ Done (18,293 tokens)
```

## What's an Aidefile?

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

Drop this into any project. That's it — it's an agent now.

## What aide handles

| Concern | Single agent | Team (HQ) |
|---------|-------------|-----------|
| **Budget** | Token limits, auto-retry | Per-agent budgets |
| **Vault** | Encrypted secrets → env vars | HQ controls who gets what |
| **Memory** | Per-agent compaction | Centralized at HQ, agents stateless |
| **Skills** | Injected at spawn | Policy controls injection |
| **Routing** | — | Policy rules or frontier fallback |
| **Telemetry** | — | Token usage, success rate, events |

## What aide does NOT do

aide doesn't replace Claude Code. Claude Code does all the thinking, coding, and reasoning. aide manages the lifecycle — who works on what, with what context, under what budget.

> Aidefile is to Claude Code what Dockerfile is to Linux.

## aide vs Claude Code native

| Feature | Claude Code (native) | aide (adds) |
|---------|---------------------|-------------|
| Run a task | `claude -p "task"` | `aide run agent "task"` — with budget + vault |
| Subagents | `.claude/agents/*.md` | `aide dispatch` — token-isolated processes |
| Memory | `~/.claude/projects/*/memory/` | HQ/memory/ — centralized SSOT |
| Secrets | env vars, manual | Vault — encrypted, gated by HQ |
| Routing | you decide | Policy — deterministic rules |

## Next steps

- [Installation](./getting-started/install.md) — get the binary
- [Quick Start](./getting-started/quickstart.md) — your first agent in 2 minutes
- [Concepts](./getting-started/concepts.md) — agents, Aidefiles, single vs team
