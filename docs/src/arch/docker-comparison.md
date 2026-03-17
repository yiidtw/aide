# Architecture: Docker Comparison

aide.sh follows Docker's conceptual model, applied to AI agents instead of application containers.

## Command mapping

| Docker | aide.sh | Purpose |
|--------|---------|---------|
| `Dockerfile` | `Agentfile.toml` | Package definition |
| `docker build` | `aide.sh build` | Create distributable image |
| `docker run` | `aide.sh run` | Create and start an instance |
| `docker exec` | `aide.sh exec` | Run a command inside an instance |
| `docker ps` | `aide.sh ps` | List running instances |
| `docker stop` | `aide.sh stop` | Stop an instance |
| `docker rm` | `aide.sh rm` | Remove an instance |
| `docker logs` | `aide.sh logs` | View instance logs |
| `docker inspect` | `aide.sh inspect` | Detailed instance metadata |
| `docker push` | `aide.sh push` | Upload image to registry |
| `docker pull` | `aide.sh pull` | Download image from registry |
| `docker images` | `aide.sh images` | List local images |
| `docker search` | `aide.sh search` | Search the registry |
| `docker login` | `aide.sh login` | Authenticate with registry |
| Docker Hub | hub.aide.sh | Public registry |
| Docker Desktop | `aide.sh dash` | GUI dashboard |

## Concept mapping

| Docker concept | aide.sh equivalent | Notes |
|----------------|-------------------|-------|
| Image | Agent type | Immutable package: Agentfile + skills + persona + seed |
| Container | Instance | Running agent with its own memory and logs |
| Volumes | `memory/` + `seed/` | `seed/` is read-only; `memory/` is read-write |
| Secrets | Vault (`~/.aide/vault.age`) | Age-encrypted, scoped per-agent and per-skill |
| Entrypoint | Skills (script or prompt) | Each skill is an independent entry point |
| ENV | `[env]` section | Declares required/optional secrets |
| Registry | hub.aide.sh | Push/pull agent images |
| Compose | `aide.toml` | Multi-agent configuration |
| Daemon | `aide.sh up` | Background process for cron + dashboard |
| CPU/RAM | LLM | Runtime resource provided by caller, not owned by agent |

## Key differences

**Multiple entry points.** A Docker container has one entrypoint. An aide.sh instance has many skills, each independently callable.

**External compute.** Docker containers own their process. aide.sh agents do not own their LLM -- it is provided by the caller (MCP client, terminal user, or local Ollama).

**Cross-tool mounting.** Docker volumes mount filesystems. `aide.sh mount` injects agent context into LLM tools (Claude Code, Codex, Gemini) as markdown files.

**Deterministic packages.** Agent images contain only scripts and markdown. No model weights, no runtime binaries, no language-specific dependencies. A typical image is kilobytes, not gigabytes.

## Lifecycle comparison

```
Docker:   build -> push -> pull -> run -> exec -> stop -> rm
aide.sh:  build -> push -> pull -> run -> exec -> stop -> rm
```

The lifecycle is intentionally identical. If you know Docker, you know aide.sh.
