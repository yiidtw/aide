# aide.sh

**Deploy AI agents, just like Docker.**

aide.sh is a CLI tool for packaging, deploying, and managing AI agents. Same agent works with or without AI — add `-p` to think.

## Why aide.sh?

- **Docker mental model** — `build`, `run`, `exec`, `push`, `pull`. If you know Docker, you know aide.sh.
- **LLM optional** — agents are structured skill runners. Add an LLM and they become autonomous reasoners.
- **Local-first** — agents run on your machine, secrets stay in your vault.
- **MCP native** — Claude, Codex, Gemini can control your agents as subagents.
- **One binary** — no Python, no Node.js, no Docker daemon.

## Quick taste

```bash
# scaffold & deploy
aide.sh init my-agent
aide.sh build my-agent/
aide.sh run my-agent --name bot

# use it
aide.sh exec bot hello world        # explicit — you drive
aide.sh exec -p bot "what's up?"    # semantic — AI drives

# monitor
aide.sh dash                        # web dashboard at localhost:3939

# expose to mobile
aide.sh expose bot telegram --token $TG_TOKEN
```

## Next steps

- [Installation](./getting-started/install.md) — get the binary
- [Quick Start](./getting-started/quickstart.md) — build your first agent in 5 minutes
- [Concepts](./getting-started/concepts.md) — images, instances, skills, vault
