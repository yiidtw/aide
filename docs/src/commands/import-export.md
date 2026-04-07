# aide import / export

Share agent templates via git.

## import

```bash
aide import <git-url>
```

Clones the repo, finds all directories with Aidefiles, copies them to `~/.aide/`, and registers them.

```bash
aide import https://github.com/org/team-agents.git
# ▸ Cloning https://github.com/org/team-agents.git...
#   ✓ Imported 'reviewer'
#   ✓ Imported 'writer'
# ✓ Imported 2 agent(s)
```

## export

```bash
aide export --to <directory> [--name <agent>]
```

| Flag | Description |
|------|-------------|
| `--to <dir>` | Output directory |
| `--name <agent>` | Only export this agent (exports all if omitted) |

Copies only shareable files: `Aidefile`, `CLAUDE.md`, `skills/`. Excludes `memory/`, `vault.*`, `.git/`.

```bash
aide export --to ./team-template
#   ✓ Exported 'reviewer'
#   ✓ Exported 'writer'
# ✓ Exported 2 agent(s) to ./team-template
```

See [Teams](../guide/teams.md) for the full workflow.
