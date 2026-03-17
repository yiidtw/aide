# aide.sh run

Create and start an agent instance from an image.

## Usage

```
aide.sh run <IMAGE> [--name NAME] [-d]
```

**IMAGE** can be:
- A local agent type defined in `aide.toml` (e.g. `jenny`)
- A registry reference in `<user>/<type>` format (e.g. `ydwu/school-assistant`)

## Options

| Flag | Description |
|------|-------------|
| `--name NAME` | Set instance name (default: `<type>.<user>`) |
| `-d, --detach` | Run in background (default for agents) |

## Examples

```bash
aide.sh run jenny                          # local type from aide.toml
aide.sh run ydwu/school-assistant          # pull from registry, then run
aide.sh run ydwu/school-assistant --name school
```

## What happens

1. Resolves the image: local `aide.toml` definition or registry pull (`<user>/<type>` format).
2. Derives instance name. Default is `<type>.<USER>` where `$USER` comes from the environment.
3. Creates the instance directory under `~/.aide/instances/<name>/` with subdirectories `memory/` and `logs/`.
4. Copies `persona.md` from the agent type if one exists.
5. Writes `instance.toml` manifest (name, type, email, role, domains, cron entries, creation timestamp).
6. Sets up cron schedules declared in the Agentfile.

## Instance directory layout

```
~/.aide/instances/<name>/
  instance.toml     # manifest
  persona.md        # copied from agent type
  Agentfile.toml    # agent package spec
  skills/           # executable skill scripts
  seed/             # read-only knowledge files
  memory/           # writable state
  logs/             # daily log files
```

## Errors

- If the instance name already exists, the command fails with a message suggesting `aide rm <name>` first.
- If the image is a registry reference that has not been pulled, it is pulled automatically.
