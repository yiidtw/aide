# Architecture: The Soul Model

An aide.sh agent does not own its LLM. The caller brings the intelligence.

## Core insight

In Docker, a container does not own the CPU. The host provides compute at runtime. aide.sh applies the same principle to AI agents: the agent defines skills, persona, and memory, but the LLM that powers reasoning is provided externally.

This means an agent can run in three modes without changing its definition:

## Three caller modes

### 1. MCP mode (frontier)

An MCP-capable client (e.g. Claude Code) calls into the agent via `aide.sh mcp`. The LLM lives on the client side. The agent's skills run as tools the LLM can invoke.

This is the highest-capability mode. The caller's frontier model handles reasoning, planning, and natural language understanding. The agent provides domain-specific actions.

### 2. Terminal mode (no LLM)

A human runs `aide.sh exec <instance> <skill> [args]` directly. No LLM is involved. The skill script executes and returns output. The human is the intelligence layer.

This is the zero-dependency mode. Every agent works here, regardless of whether an LLM is available.

### 3. Daemon mode (local model)

The `aide.sh up` daemon runs scheduled tasks using a local model via Ollama. The `[soul]` section in `Agentfile.toml` declares preferences:

```toml
[soul]
prefer = "llama3.2:3b"
min_params = "1b"
```

- `prefer` -- the preferred local model identifier.
- `min_params` -- minimum model size the agent needs.

The daemon selects the best available model that meets the requirement.

## Docker analogy

| Docker | aide.sh |
|--------|---------|
| Container does not own CPU | Agent does not own LLM |
| Host provides compute | Caller provides intelligence |
| Works on any host with Docker | Works with any LLM (or none) |
| CPU is a runtime resource | LLM is a runtime resource |

## Design consequence

Because the LLM is external, agent packages are small and deterministic. A skill is a bash script. A persona is a markdown file. There are no model weights, no inference servers, no GPU requirements in the agent image.

The same agent image that a frontier model orchestrates via MCP can also be operated by a human typing commands in a terminal.
