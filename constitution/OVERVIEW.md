# Overview

> **aide.sh — Deploy AI agents, just like Docker.**

## What is aide.sh

Deploy AI agents, just like Docker. Package agents with Agentfile,
run them across machines, manage credentials with aide vault,
discover them via MCP.

## The thesis

```
agent = trust(skill) + feedback(memory)
```

- A skill without trust is a script.
- Trust without feedback is static analysis.
- All three together make an agent — it can act, it is constrained, it evolves.

## What aide.sh is NOT

- Not an email agent (email is one interface)
- Not a chatbot (agents act autonomously within trust boundaries)
- Not a SaaS product yet (personal tool first, product later)

## Design principles

1. **Data integrity over convenience** — machines are means, consistency is the goal
2. **Formal trust over vibes** — every guarantee is backed by verification
3. **Local-first** — your data stays on your machines, cloud is optional sync
4. **Automate everything** — manual O(1) is the limit
5. **Constitution-driven** — code follows spec, not the other way around

## Product ecosystem — 5 agent PMs, 8 domains

Each product vertical has an agent PM (a container in the Agent OS).
The CEO receives daily morning reports from each PM via email.

| Agent | Domains | Vertical |
|-------|---------|----------|
| **Jenny** | aide.sh | Agent OS core, orchestrator, default handler |
| **Henry** | example.com | Customized RAG + Zotero, user-space memory |
| **Terry** | example.com, example.com | Payment layer (apay) + video understanding pipeline |
| **Paul** | example.com, example.com | Marketing channel + agent collaboration frontend |
| **Amber** | example.com, example.com | Carbon commerce + web3 loyalty (dormant) |

## Interaction model — caller pattern

General-purpose sessions (Claude Code, Codex) should NOT do domain work
directly. Instead, they delegate to specialized agent instances:

```
User → General session → aide call jenny.ydwu cool scan → Jenny executes → result
```

**Three roles:**
- **Caller** (general session): understands user intent, routes to agent
- **Agent** (Jenny, Henry...): persistent instance with skills, memory, cron
- **Skill** (wonskill): immutable, versioned code that does the actual work

**Discovery:** Callers discover agents via MCP (`aide mcp-server`) or hooks.
When a caller detects domain-relevant keywords (e.g. "NTU", "email",
"COOL"), it should route to the appropriate agent instead of improvising.

## Skill immutability

Skills are immutable artifacts — like Docker image layers:

```
wonskill build cool → cool-0.2.0.tar.gz (versioned, checksummed)
aide push cool-0.2.0  → registry (CF Worker / R2)
aide pull cool-0.2.0  → installed on target machine
```

- Skills are **never modified at runtime** by agents
- Updates require explicit version bump + rebuild + push
- Trust verification (galv) runs at build time, result is baked into artifact
- This guarantees: agent behavior = f(immutable skill, mutable memory)

**Consequence:** if an agent misbehaves, the root cause is always in memory
(fixable by feedback) — never in skill code (which is verified + frozen).

## Key decisions

- **aide.sh** is the brand (not ydwu.dev — must scale beyond one person)
- **Rust** for the daemon (formally verifiable, single binary)
- **aide.toml** is the unified config file
- **your-server** is the always-on server, local Mac is dev
- Sub-components (galv, arun, amem, apay) are crates, not separate products
- **example.com = apay** — web3 payment layer, example.com is first customer
- **example.com** — video understanding pipeline, first revenue product
- **Caller pattern** — general sessions delegate to agents, not DIY
- **Skill immutability** — versioned artifacts, never runtime-modified
