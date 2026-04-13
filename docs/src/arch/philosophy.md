# Philosophy

## What aide is (and isn't)

aide is a **commander** — it manages agents, not replaces them. Claude Code does all the thinking. aide decides who works on what, with what context, under what budget.

### What's Claude Code (native)

These features exist without aide:

- **`claude -p`** — headless Claude Code, runs a task and exits
- **`.claude/agents/`** — custom subagent definitions, Claude Code dispatches via Agent tool
- **Auto-memory** — Claude Code's built-in per-project memory in `~/.claude/projects/`
- **CLAUDE.md** — project-level instructions Claude Code reads on startup
- **Hooks** — PreToolUse, PostToolUse, PreCompact lifecycle events

Claude Code is already a powerful single-agent runtime. A single agent doesn't need a framework.

### What aide adds

aide's value shows up when you have **multiple agents** that need to coordinate:

| Problem | aide's answer |
|---------|---------------|
| Token explosion — frontier context grows with every subtask | `aide dispatch` runs work in isolated `claude -p` processes |
| Secrets scattered across projects | Vault — centralized, encrypted, injected at spawn time |
| Each agent remembers different things | HQ memory — single source of truth, agents are stateless |
| No visibility into what agents are doing | Telemetry, events timeline, dashboard |
| Manual routing — you decide which agent gets which task | Policy — deterministic rules, or frontier fallback |
| Skill bloat — injecting everything wastes context | Policy controls which skills get injected per task |

### What aide does NOT do

- Replace Claude Code's reasoning, planning, or coding
- Provide its own LLM runtime
- Require you to write Python/TypeScript glue code
- Run containers or cloud infrastructure

## Aidefile is to Claude Code what Dockerfile is to Linux

A Dockerfile doesn't replace Linux. It declares how to package and run a process on top of Linux. An Aidefile doesn't replace Claude Code. It declares how to package and run an agent on top of Claude Code.

| Dockerfile | Aidefile |
|-----------|----------|
| FROM, RUN, COPY | [persona], [skills] |
| ENV | [vault] |
| HEALTHCHECK | [budget] |
| ENTRYPOINT | [trigger] |

A single Aidefile is all you need. Drop it into any project — public or private — and it becomes an agent.

## Two layers

**Layer 1: Aidefile (single agent)**

Any project with an Aidefile is an agent. `aide run` gives it budget control, vault injection, memory compaction. No HQ, no daemon, no orchestration. This is all most people need.

**Layer 2: HQ (multi-agent)**

When you have multiple Aidefiles, `aide init --team` creates a coordinator repo (HQ) that manages:

- **Dispatch** — route tasks to the right agent
- **Memory** — centralized team memory, agents are stateless
- **Policy** — deterministic routing rules + skill/vault gating
- **Telemetry** — token usage, success rate, routing decisions

Layer 2 builds on Layer 1. Every agent in a team is still just a directory with an Aidefile.

## The token problem

A frontier Claude Code session has a finite context window. Every subtask you handle inline eats tokens and pushes older context out. With 5 agents doing 50k tokens each, your frontier burns 250k tokens of context — most of which is irrelevant to the next task.

aide solves this by **process isolation**: `aide dispatch` spawns a separate `claude -p` process with its own context window. The frontier only sees the bounded summary. 50k tokens of agent work compresses to ~500 tokens of output.

This is why aide exists. Not to be a framework, but to **save tokens** through isolation.

## Commander, not framework

aide is your commander of agents. You give orders (`aide dispatch`), aide handles the rest — who gets the task, what context they need, what secrets they get, how much they can spend. Agents do the work and report back. You check results when you're ready.

```
You → Claude Code (frontier) → aide dispatch → agent (claude -p) → output
         ↑                         ↑
     your session            aide binary handles:
     stays lean              vault, memory, skills,
                             policy, telemetry
```
