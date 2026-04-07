# Philosophy

## Claude Code is the runtime

Claude Code is already a powerful agent runtime — it can read files, write code, run commands, and reason about complex tasks. aide doesn't try to replicate or replace any of that.

aide is the **lifecycle manager**. It answers four questions:

1. **Who to wake up** — agent registry and trigger dispatch
2. **How much to spend** — token budget enforcement
3. **What secrets to give** — vault injection at spawn time
4. **When to stop** — budget exhaustion and retry limits

Everything else — the actual thinking, coding, planning — is Claude Code's job.

## Aidefile is to Claude Code what Dockerfile is to Linux

A Dockerfile doesn't replace Linux. It declares how to package and run a process on top of Linux. Similarly, an Aidefile doesn't replace Claude Code. It declares how to package and run an agent on top of Claude Code.

| Dockerfile | Aidefile |
|-----------|----------|
| FROM, RUN, COPY | [persona], [skills] |
| ENV | [vault] |
| HEALTHCHECK | [budget] |
| ENTRYPOINT | [trigger] |

## Minimal by design

aide is ~1000 lines of Rust. No framework, no containers, no runtime dependencies beyond Claude Code itself.

The agent is just a directory with an Aidefile. `aide run` is just `claude -p` with a budget wrapper and vault injection. The simplicity is the point — fewer moving parts means fewer things to break.

## Fire and forget

The ideal aide workflow is:

1. Write an Aidefile
2. `aide up`
3. Walk away

Agents wake up on triggers, do their work within budget, and go back to sleep. You check the results when you're ready.
