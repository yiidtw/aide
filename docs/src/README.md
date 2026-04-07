# aide.sh

**One file to agentize your Claude project.**

aide is a lifecycle manager for Claude Code agents. Drop an `Aidefile` into any Claude Code project — it becomes an agent with a persona, budget, vault, hooks, and triggers.

## Why aide?

- **Minimal** — one Rust binary, one config file. No runtime, no containers, no framework.
- **Claude Code native** — `aide run` calls `claude -p` under the hood. All intelligence is in Claude Code.
- **Fire and forget** — set a token budget, define triggers, and walk away. aide handles the rest.
- **Secure** — age-encrypted vault secrets injected as env vars at spawn time. Never enters the LLM context window.

## Quick taste

```bash
# Install
cargo install aide-sh

# Turn any Claude project into an agent
cd ~/projects/code-reviewer
aide init --persona "Senior Reviewer"
# ✓ Created Aidefile

# One-shot: fire and forget
aide run . "Review PR #42 and leave comments"
# ▸ Running task in ~/projects/code-reviewer
#   agent: Senior Reviewer
#   budget: 200000 tokens
# ✓ Task completed (23,847 tokens used)

# Or register and run by name
aide register . --name reviewer
aide run reviewer "Review all open PRs"

# Start daemon: agents wake up on triggers
aide up
```

## What is an Aidefile?

```toml
[persona]
name = "Senior Reviewer"
style = "direct, cares about edge cases"

[budget]
tokens = "100k"
max_retries = 3

[hooks]
on_spawn = ["inject-vault"]
on_complete = ["commit-memory"]

[trigger]
on = "issue"

[vault]
keys = ["GITHUB_TOKEN"]
```

That's it. Your Claude project is now an agent.

## What aide handles

| Feature | Description |
|---------|-------------|
| **Budget** | Token limits per task. Auto-retry with remaining budget. |
| **Vault** | age-encrypted secrets injected as env vars at spawn time. |
| **Triggers** | GitHub Issues, cron, manual. Daemon polls and dispatches. |
| **Hooks** | on_spawn, on_complete. Inject vault, commit memory, notify. |
| **Memory** | Auto-compact when threshold is hit. Per-agent memory namespace. |
| **Teams** | Import/export agent templates via git. |

## Philosophy

Claude Code is already a great agent runtime. aide doesn't replace it.

aide is the **lifecycle manager** — who to wake up, how much to spend, what secrets to give, when to stop.

> Aidefile is to Claude Code what Dockerfile is to Linux.

## Next steps

- [Installation](./getting-started/install.md) — get the binary
- [Quick Start](./getting-started/quickstart.md) — your first agent in 2 minutes
- [Concepts](./getting-started/concepts.md) — agents, Aidefiles, registry
