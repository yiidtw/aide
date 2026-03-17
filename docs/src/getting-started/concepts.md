# Concepts

Core ideas behind aide.sh, mapped to Docker equivalents where applicable.

## Images vs Instances

| Docker | aide.sh | Description |
|--------|---------|-------------|
| Image | Image | Immutable snapshot built from Agentfile.toml |
| Container | Instance | Running copy of an image with its own state |
| Dockerfile | Agentfile.toml | Declarative manifest |
| docker build | aide.sh build | Package into image |
| docker run | aide.sh run | Create instance from image |
| docker exec | aide.sh exec | Run a command inside instance |

Images live in `~/.aide/images/`. Instances live in `~/.aide/instances/`.

## Agentfile.toml

The manifest that defines an agent. Contains:

- **[agent]** — name, version, description, author
- **[persona]** — pointer to a markdown file describing the agent's identity
- **[skills.NAME]** — executable capabilities (scripts or prompts)
- **[seed]** — static data bundled into the image
- **[env]** — required and optional environment variables
- **[soul]** — LLM routing preferences

## Skills

A skill is a named capability. Two types:

- **Script-based** — a shell script (`skills/hello.sh`) that receives args via `$1`, `$2`, etc.
- **Prompt-based** — a markdown file (`skills/summarize.md`) interpreted by an LLM at runtime.

Skills are invoked with `aide.sh exec <instance> <skill> [args...]`.

## Persona

A markdown file that describes who the agent is. Used by LLMs when the agent runs in semantic mode. Has no effect in explicit (non-LLM) mode.

## Vault

Encrypted secret storage. Secrets are injected as environment variables at skill execution time. Three-tier scoping:

1. **Per-skill env** — highest priority, set in `[skills.NAME] env`
2. **Per-agent env** — set in `[env]`
3. **Vault** — global secrets, lowest priority

See [Vault & Secrets](../guide/vault.md) for details.

## Semantic injection (the `-p` flag)

By default, `aide.sh exec` runs skills explicitly -- you name the skill and pass args.

Add `-p` and the input becomes a natural language prompt routed through an LLM:

```bash
# explicit: you pick the skill and args
aide.sh exec bot email check

# semantic: LLM picks the skill and args
aide.sh exec -p bot "do I have new mail?"
```

The agent itself is unchanged. The `-p` flag wraps it with an LLM reasoning layer. Without `-p`, no LLM is involved -- the human acts as the reasoning layer.

## MCP (Model Context Protocol)

MCP lets LLM hosts (Claude Code, Cursor, etc.) call aide.sh agents as tools. Running `aide.sh setup-mcp` registers your agents so that an LLM can:

- List available agents and skills
- Execute skills and read output
- Access logs

See [MCP Integration](../guide/mcp.md) for setup instructions.
