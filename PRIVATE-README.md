# aide-private — Feature Development

> **aide.sh** — Deploy AI agents, just like Docker.

This is the private development repo for [aide.sh](https://github.com/yiidtw/aide).
Public releases are pushed to `yiidtw/aide`.

## What is aide.sh

A CLI tool (Rust) that brings Docker semantics to AI agents:
- `aide pull` / `aide run` / `aide exec` / `aide ps` / `aide rm`
- Each agent = persona + skills + memory, running as `claude -p` in subscription mode
- Skills are bash scripts; memory is git-backed; secrets live in age-encrypted vault

Published on [crates.io](https://crates.io/crates/aide-sh) as `aide-sh` (binary: `aide`).

## Architecture

```
aide binary (Rust)        — infra: vault, daemon, deploy, cron
aide MCP server           — agent ops: exec, commit, logs (structured JSON)
aide-skill CLI            — 45+ utility skills (web, email, cloudflare, etc.)
```

### Core Rules (TLA+ verified)
1. Every aide = isolated `ANTHROPIC_API_KEY="" claude -p` (subscription mode by default)
2. Router can only Dispatch/Merge/Spawn — never Execute directly
3. All agents must have post-task hook → update memory or skill

## Current Direction: aide as a Service

**Problem:** Non-engineers don't know what skill-based agents are, but need them.
Claude cowork/dispatch UX is bad (sandbox limits, env isolation). n8n/Zapier requires users to design workflows.

**aide's approach:** User says *what* they want → agent decides *how*.

### MVP
```
User (PWA/email)
  → Paste API key (aide vault, age-encrypted)
  → Connect Google Drive (OAuth)
  → Pick a skill or describe task
  → CF Worker → aide exec → result back to user
```

- User pays LLM tokens (their API key), we pay zero LLM cost
- We provide infra (CF Workers free tier)
- Vault runtime inject, destroy after use (Docker pattern)

### Two tracks
- **Open source**: Engineers run aide themselves
- **Hosted**: Non-engineers pay for fully managed service

## Agents (11 total)

| Agent | git-native | GitHub repo |
|-------|------------|-------------|
| agentbelt-demo.yiidtw | YES | aide-agentbelt-demo |
| agentbelt.yiidtw | YES | aide-agentbelt |
| twitter.yiidtw | YES | aide-twitter |
| gmail.yiidtw | YES | — |
| teaching-monster.yiidtw | YES | — |
| crate-publish.yiidtw | NO | — |
| cve.ydwu | NO | — |
| devops.yiidtw | NO | aide-devops |
| easychair.yiidtw | NO | — |
| ntu.yiidtw | NO | — |
| paperreview.yiidtw | NO | — |

## Infra

- **Local Mac**: dev, aide-skill, Chrome debug port [::1]:9222
- **formace-00**: RTX 6000 Ada, aide + aide-skill + aide-gaia
- **Vault**: age-encrypted, git-synced (SSOT)

## Related Repos

- [yiidtw/aide](https://github.com/yiidtw/aide) — public release
- yiidtw/aide-gaia — GAIA benchmark evaluation (EMNLP 2026 target)
- yiidtw/aide-twitter, aide-agentbelt, aide-devops — agent instance repos
