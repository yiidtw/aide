# aide spawn

Create a new agent under `~/.aide/`.

## Usage

```bash
aide spawn <name> [--persona <persona>]
```

## Options

| Flag | Description |
|------|-------------|
| `--persona <name>` | Persona name for the Aidefile. Defaults to the agent name. |

## Example

```bash
aide spawn reviewer --persona "Senior Reviewer"
# ✓ Spawned agent 'reviewer' at ~/.aide/reviewer
#   Edit ~/.aide/reviewer/Aidefile to configure
```

## Created structure

```
~/.aide/reviewer/
├── Aidefile
├── CLAUDE.md
├── memory/
└── skills/
```

The agent is automatically registered in the registry. Errors if an agent with the same name already exists.
