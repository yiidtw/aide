# Teams (Import / Export)

aide supports sharing agent templates via git repositories.

## Export

```bash
# Export all agents
aide export --to ./my-team-template

# Export a specific agent
aide export --to ./my-team-template --name reviewer
```

Export copies only shareable files:
- `Aidefile`
- `CLAUDE.md`
- `skills/`

It **excludes** private data:
- `memory/` — per-deployment state
- `vault.key` — private encryption key
- `vault.age` — encrypted secrets
- `.git/`

## Import

```bash
aide import https://github.com/user/agent-templates.git
```

Import clones the repo and registers every subdirectory that contains an Aidefile. Agents are copied to `~/.aide/<name>/`.

If the repo root itself has an Aidefile, it's also imported (named after the repo).

## Team workflow

```
team-agents/           ← git repo
├── reviewer/
│   ├── Aidefile
│   ├── CLAUDE.md
│   └── skills/
├── writer/
│   ├── Aidefile
│   ├── CLAUDE.md
│   └── skills/
└── README.md
```

```bash
# Team lead exports
aide export --to ./team-agents
cd team-agents && git push

# Team member imports
aide import https://github.com/org/team-agents.git
# ✓ Imported 'reviewer'
# ✓ Imported 'writer'
```

Each member sets up their own vault locally. The templates are shared; the secrets are not.
