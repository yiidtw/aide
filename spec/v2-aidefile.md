# aide v2 — One file to agentize your Claude project

## Core Thesis

A Claude Code project + `Aidefile` = agent.  
`Aidefile` is the **only** file aide cares about.  
aide binary is a **thin lifecycle manager** — it spawns, budgets, and delivers.

## What aide binary does

```
while alive:
  1. poll triggers (GitHub Issues, cron, webhook)
  2. match trigger → find Aidefile directory
  3. parse Aidefile → budget, vault, hooks, skills
  4. on_spawn hooks (vault injection as env vars)
  5. claude -p "<task>" in that directory
  6. monitor token usage against budget
  7. on_complete hooks (commit memory, notify, close issue)
  8. if not done && budget remaining → loop to 5
```

## Commands

```
aide run <path> "<task>"       # one-shot: find Aidefile, execute task
aide spawn <name>              # create ~/.aide/<name>/ with Aidefile template
aide register <path>           # add existing project to managed list
aide list                      # show all managed agents (name, path, status)
aide up                        # start daemon (trigger polling loop)
aide down                      # stop daemon
aide import <url>              # git clone + auto-register all Aidefiles
aide export --to <path>        # copy Aidefile + CLAUDE.md + skills/, strip vault/memory
aide config                    # edit ~/.aide/config.toml
```

## Aidefile spec

TOML format. Every field optional except `[persona].name`.

```toml
[persona]
name = "Senior Reviewer"
style = "direct, terse, cares about edge cases"

[budget]
tokens = "100k"             # max tokens per task invocation
max_retries = 3             # max re-invocations if task incomplete

[memory]
compact_after = "200k"      # auto-compact threshold

[hooks]
on_spawn = ["inject-vault"]
on_complete = ["commit-memory"]

[skills]
include = ["code-review", "test"]

[trigger]
on = "issue"                # "issue" | "cron:EXPR" | "webhook:URL" | "manual"

[vault]
keys = ["GITHUB_TOKEN", "OPENAI_API_KEY"]  # required env vars from vault
```

## ~/.aide/ layout

```
~/.aide/
  config.toml              # daemon config + agent registry
  vault.age                # encrypted secrets (age)
  vault.key                # per-machine private key
  daemon.pid               # running daemon PID
  reviewer/                # aide spawn'd agent
    Aidefile
    CLAUDE.md
    memory/
    skills/
```

## config.toml

```toml
[daemon]
poll_interval = "60s"

[[agents]]
name = "reviewer"
path = "~/.aide/reviewer"

[[agents]]
name = "homework-bot"
path = "~/projects/homework-bot"
```

## Vault injection

Secrets are **never** placed in the LLM prompt or context window.

1. aide reads `[vault].keys` from Aidefile
2. Decrypts matching keys from `~/.aide/vault.age`
3. Sets them as environment variables before spawning `claude -p`
4. claude -p's tools/hooks can read env vars, but the LLM itself cannot

**Kani invariant**: `vault_inject()` output is `Vec<(String, String)>` passed only to
`std::process::Command::env()`, never to the prompt string.

## Token budget enforcement

aide wraps `claude -p` and monitors output for token usage.

**Kani invariant**: `budget.tokens` is parsed to `u64`. Each invocation's reported
usage is accumulated. Next invocation is blocked if `accumulated >= budget`.

## Memory compaction

After each task completes, aide checks memory directory size.
If total tokens in memory/ exceed `compact_after`, aide runs:
```
claude -p "Compact your memory. Summarize and deduplicate." --cwd <agent_dir>
```

This counts against a separate internal budget, not the task budget.

## Trigger dispatch (daemon mode)

### GitHub Issues
- Poll issues assigned to agent (label or assignee match)
- New issue → `aide run <path> "<issue title + body>"`
- On complete → close issue with result comment

### Cron
- Parse cron expression from `[trigger].on`
- At trigger time → `aide run <path> "<skill or default task>"`

### Webhook
- Expose HTTP endpoint (aide up --port)
- POST with JSON body → route to matching agent

## Migration from v1

v1 instances in `~/.aide/instances/` are left untouched.
v2 agents are registered in `config.toml`. No migration required —
users can gradually `aide register` old projects after adding an Aidefile.

## Kani verification targets

1. **aidefile_parse**: valid TOML → `Aidefile` struct. All optional fields default safely.
2. **vault_inject**: secrets flow only to env vars, never to prompt string.
3. **budget_check**: accumulated tokens never exceed budget before next spawn.
4. **trigger_match**: each trigger event dispatches to exactly one agent or none.
5. **daemon_loop**: daemon always terminates cleanly on SIGTERM (no orphan claude -p).

## Non-goals

- LLM inference (Claude does this)
- Knowledge management (crossmem does this)
- Skill execution runtime (claude -p does this)
- Team hierarchy / routing (just use multiple Aidefiles)
- Web dashboard (overkill for v2; add later if needed)

## Dependencies (minimal)

- `clap` — CLI
- `toml` + `serde` — Aidefile parsing
- `tokio` — async daemon loop
- `age` — vault decryption (via `rage` crate or shell)
- `reqwest` — GitHub API polling
- `rusqlite` — agent registry (or just config.toml)

No axum, no ratatui, no dashboard, no hub protocol.
