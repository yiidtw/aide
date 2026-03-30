# Roadmap: From Local Daemon to Edge Runtime

aide's architecture is designed to evolve from a local daemon to a globally distributed edge runtime without rewriting the core abstractions. This document traces that path.

## Current State

```
aide binary (Rust)
  ├── CLI         — create / run / exec / deploy / mount
  ├── MCP server  — Claude Code integration (aide_exec, aide_list, etc.)
  ├── Daemon      — cron ticker, Gmail poller, Telegram bot, GitHub issues
  ├── Vault       — age-encrypted secret management
  └── Git ops     — auto-commit, push, sanity check

Instance dir (~/.aide/instances/<name>/)
  ├── cognition/  — memory/, logs/ (agent state)
  ├── occupation/ — skills/, persona.md, knowledge/, Agentfile.toml (copied from type)
  └── instance.toml
```

Everything runs on a single machine. The aide binary is daemon + CLI + MCP + vault + git in one process.

## Phase 1: Clean Separation (in progress)

**Goal**: instance dir = pure state, occupation = injected at runtime.

| Issue | What | Status |
|-------|------|--------|
| [#40](https://github.com/yiidtw/aide-private/issues/40) | Skill injection — resolve occupation from agent type at runtime, not copy | planned |
| [#39](https://github.com/yiidtw/aide-private/issues/39) | Daily cognition commit — only `cognition/` auto-committed | done |
| [#38](https://github.com/yiidtw/aide-private/issues/38) | Router delegates tasks to aide, not skills | planned |
| [#1](https://github.com/yiidtw/aide-private/issues/1) | Pre-#72 migration — all instances get .git | planned |

After Phase 1:

```
Instance dir = cognition only (git repo)
Agent type   = occupation (read-only image, shared across instances)
Vault        = injected as env vars at exec time
```

This is the Docker model: image (read-only) + container (state) + volume mount (secrets).

## Phase 2: TypeScript-Only Skills

**Goal**: single runtime, portable, edge-compatible.

| Issue | What |
|-------|------|
| [#43](https://github.com/yiidtw/aide-private/issues/43) | Deprecate bash skills, enforce TypeScript for all new skills |

Why:
- Bash can't run on edge runtimes (Cloudflare Workers, Deno Deploy)
- No type safety, no imports, no testability
- Agents default to writing bash — fragile, non-portable
- Can't share a runtime SDK across bash scripts

After Phase 2:

```
Skills = .ts only (runs on bun locally, edge runtime remotely)
@aide-sh/runtime SDK:
  - memory.read() / memory.write()  — cognition access
  - vault.get()                     — secret access
  - log.info() / log.error()        — structured logging
```

## Phase 3: Edge Deployment

**Goal**: `aide deploy --edge` ships an instance to a serverless platform.

| Issue | What |
|-------|------|
| [#42](https://github.com/yiidtw/aide-private/issues/42) | Edge function deployment target |

### The aide binary splits

```
Local:   aide binary = daemon + CLI + MCP + vault + git (single Rust binary)
Cloud:   aide CLI    = deploy tool only (bundle occupation, set triggers, push to platform)
         @aide-sh/runtime = TS library (runs inside edge function)
```

The Rust binary does NOT run on cloud. It's a deploy-time tool, like `wrangler` or `vercel` CLI.

### Local → Edge mapping

| Local (daemon) | Edge function | Trigger |
|---|---|---|
| Cron ticker | Scheduled worker | Platform cron |
| GitHub poller | HTTP function | GitHub webhook |
| Telegram bot | HTTP function | Telegram webhook |
| Gmail poller | HTTP function | Google Pub/Sub |
| `aide exec` | HTTP function | HTTP POST |

### Edge function lifecycle

```
1. Trigger (cron / webhook / HTTP)
2. Load cognition (git shallow clone, or KV/R2 cache)
3. Resolve skill from bundled occupation (deployed with the function)
4. Load secrets from platform env vars (= vault)
5. Execute skill (.ts)
6. Commit cognition (git push, or KV/R2 write-through)
7. Return response
```

### aide binary's role on cloud

The Rust binary stays on your machine as a long-running daemon. It does NOT run on edge. Its role changes from "runtime" to "control plane":

| Responsibility | Local (runtime) | Cloud (deploy-time) |
|---|---|---|
| Skill execution | Direct exec | Bundle → deploy to platform |
| MCP server | stdio JSON-RPC | N/A (HTTP invocation) |
| Cron scheduling | Daemon ticker | Configure platform triggers |
| Vault | Decrypt at runtime | Push secrets at deploy-time |
| Git ops | auto_commit + push | N/A (runtime SDK handles) |

### Vault on cloud

vault.age remains single source of truth, but its role splits:

| | Local | Cloud |
|---|---|---|
| Storage | `~/.aide/vault.age` (age encrypted) | Platform secret store |
| Decryption | aide binary, every exec | Not needed — platform manages |
| Injection | Runtime, before each skill | Deploy-time, aide CLI pushes once |
| Update | `aide vault set KEY` | `aide deploy --sync-secrets` |

```bash
aide deploy --edge cloudflare twitter.ydwu
# 1. Read Agentfile.toml → identify required env vars
# 2. Decrypt vault.age → extract only needed vars
# 3. wrangler secret put TWITTER_API_KEY <value>
# 4. Bundle occupation/*.ts → wrangler deploy
# 5. Configure cron triggers from Agentfile [skills.*.schedule]
```

### Platform targets

1. **Cloudflare Workers** — KV/R2 for cognition cache, Cron Triggers, best edge runtime
2. **Deno Deploy** — native TS, good DX
3. **Vercel Edge Functions** — wide adoption
4. **AWS Lambda@Edge** — enterprise

### Cognition persistence on edge

```
Source of truth:  git repo (GitHub)
Hot cache:        KV store (platform-native) or R2/S3

Read path:   KV cache hit → return
             KV cache miss → git clone --depth 1 → populate KV → return

Write path:  Write to KV (immediate)
             git add + commit + push (async, batched)
```

## Phase 4: Agent-to-Agent Communication

Once instances run on edge, they can call each other via HTTP:

```
aide instance A (Cloudflare Worker, us-east)
  → HTTP POST /exec to aide instance B (Deno Deploy, eu-west)
  → B reads its cognition, executes skill, commits, responds
  → A incorporates B's response into its own cognition
```

This is where `aide_task` (#38) becomes critical — A doesn't call B's specific skill, it sends a task and lets B reason about how to execute it.

## Dependency Graph

```
#1  (migration: .git for all instances)
 └── #39 (daily cognition commit) ✅ done
      └── #40 (skill injection)
           └── #43 (TS-only skills)
                └── #42 (edge deployment)
                     └── #38 (aide_task: agent-to-agent delegation)
                          └── #4  (GAIA benchmark: prove multi-agent > single agent)
```

## Competitive Landscape

See [#41](https://github.com/yiidtw/aide-private/issues/41) for full analysis. Key insight:

| Competitor | Runs on | Persistence | aide's edge |
|---|---|---|---|
| Claude Agent Teams | Local (Claude Code sessions) | Filesystem, ephemeral | Git-native, survives sessions |
| Paperclip | Self-hosted (Node.js + PostgreSQL) | PostgreSQL | No database required |
| CrewAI | Local (Python process) | None (stateless) | Persistent memory across runs |
| OMC | Local (Claude Code plugin) | None | Independent of any single LLM |
| Ruflo | Self-hosted (Node.js + PostgreSQL + WASM) | PostgreSQL + vector DB | Lighter, git-only |

aide is the only tool where an agent's entire lifecycle — creation, execution, memory, skills — lives in git. This makes edge deployment natural: git is already distributed.
