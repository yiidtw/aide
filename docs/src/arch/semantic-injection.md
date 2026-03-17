# Architecture: Agent Execution Modes

aide.sh agents have three execution modes. Same agent, same skills — different brains.

## Three Modes

### 1. Ad-hoc (no LLM)

```bash
aide.sh exec school pr list
aide.sh exec school notifications
```

Human directly calls a skill by name. Script runs, output returns. No AI involved.
This is like calling a Docker container's command directly.

### 2. Sub-agent (caller's LLM)

```
Claude Code / Codex / Gemini
  └── MCP: aide_exec(school, pr, list)
```

A frontier CLI model controls the agent via MCP. The caller's LLM decides what
skills to invoke. The agent doesn't need its own brain — the caller IS the brain.

### 3. Standalone (agent has its own LLM)

```bash
aide.sh exec -p school "what's due this week?"
```

The `-p` flag gives the agent a soul. The query is piped through an LLM
(`claude -p` or local ollama) which reads the persona, examines available skills,
and autonomously decides what to call.

```
human → LLM (claude -p / ollama) → selects skill → executes → LLM → human
```

## How `-p` works

1. Reads `persona.md` — who the agent is
2. Reads skill catalog — what it can do (names, descriptions, usage)
3. Composes a prompt: "Given these skills, answer this query"
4. LLM selects skill + args
5. aide.sh executes the skill
6. LLM formats the output for the human

## LLM Resolution Order

When `-p` is used, aide.sh looks for an LLM in this order:

1. `claude -p` — if Claude Code CLI is installed
2. `ollama` — if a local model is available
3. `[soul]` section in Agentfile.toml — preferred model hint

```toml
[soul]
prefer = "llama3.2:3b"
```

## Comparison

| | Ad-hoc | Sub-agent | Standalone |
|---|---|---|---|
| Who decides | Human | Caller's LLM | Agent's LLM |
| Trigger | `aide.sh exec` | MCP `aide_exec` | `aide.sh exec -p` |
| LLM needed | No | Caller has it | Yes (claude/ollama) |
| Offline | Yes | No | Depends on LLM |
| Use case | Scripts, cron, CI | Claude Code workflows | Chat, email, Telegram |

## Why this matters

**LLM is a runtime, not a dependency.** An agent that works in ad-hoc mode will
always work — on any machine, offline, without API keys. The `-p` flag and MCP
are enhancements, not requirements.

This is the opposite of frameworks where the LLM is baked into the agent. In aide.sh,
the agent is a set of capabilities. The LLM — whether frontier or local — is an
optional brain that makes those capabilities accessible via natural language.
