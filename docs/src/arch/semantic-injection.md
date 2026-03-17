# Architecture: Semantic Injection

Same agent, with or without AI. Add `-p` to think.

## Two execution modes

Every aide.sh agent supports two ways of being called:

### Explicit mode (default)

```bash
aide.sh exec school cool courses
aide.sh exec school email unread
aide.sh exec school cool assignments
```

The caller names the exact skill and passes structured arguments. The skill script runs directly. No LLM is involved.

### Semantic mode (`-p` flag)

```bash
aide.sh exec -p school "what's due this week?"
aide.sh exec -p school "check if any professors replied"
aide.sh exec -p school "summarize my grades"
```

The `-p` flag activates semantic injection. The natural language query is routed through an LLM, which selects the appropriate skill(s) and arguments based on the agent's persona and skill descriptions.

## How semantic mode works

1. The query and the agent's skill catalog (names, descriptions, usage strings) are composed into a prompt.
2. The LLM (caller-provided or local Ollama) interprets the query and maps it to one or more skill invocations.
3. The skill scripts execute normally.
4. Results are returned, optionally summarized by the LLM.

The skill scripts themselves are unchanged between modes. Semantic mode wraps the dispatch layer, not the execution layer.

## Why this matters

**LLM is a runtime, not a dependency.** An agent that works in explicit mode will always work -- on any machine, offline, without API keys. Semantic mode is an enhancement, not a requirement.

This is the opposite of frameworks where the LLM is baked into the agent definition. In aide.sh, the agent is a set of capabilities. The LLM is an optional accelerator that makes those capabilities accessible via natural language.

## Comparison with other frameworks

| Property | aide.sh | LangChain / CrewAI / AutoGen |
|----------|---------|------------------------------|
| LLM required? | No (explicit mode works without) | Yes (core dependency) |
| Skill definition | Bash scripts + markdown prompts | Python functions + LLM chains |
| Offline capable? | Yes (explicit mode) | No |
| Human fallback? | Human is the LLM in terminal mode | No equivalent |
| Package size | KB (scripts + markdown) | MB+ (Python deps + model configs) |

## The `-p` mental model

Think of `-p` as "pipe through intelligence." Without it, you talk to the agent in its native protocol (skill names + args). With it, you talk in natural language and the LLM translates.

```
Without -p:  human -> skill -> output
With -p:     human -> LLM -> skill -> output -> LLM -> human
```
