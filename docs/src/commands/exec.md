# aide.sh exec

Execute a skill on a running agent instance.

## Usage

```
aide.sh exec [FLAGS] <INSTANCE> [SKILL] [ARGS...]
```

When called without a skill, lists all available skills for the instance (equivalent to `--help`).

## Options

| Flag | Description |
|------|-------------|
| `-i, --interactive` | Interactive mode (allocate pseudo-TTY) |
| `-t, --tty` | Allocate pseudo-TTY |

## Examples

```bash
aide.sh exec reviewer.demo                   # list available skills
aide.sh exec reviewer.demo pr list           # run the "pr" skill with arg "list"
aide.sh exec reviewer.demo notifications     # check notifications
aide.sh exec -it reviewer.demo diff          # interactive mode
```

## Skill resolution

1. Looks up the instance under `~/.aide/instances/<instance>/`.
2. Loads `Agentfile.toml` from the instance directory.
3. Finds the skill definition and locates the script at `skills/<skill>.sh`.
4. Executes the script via `bash`, passing remaining arguments.

## Environment scoping

Secrets from the vault (`~/.aide/vault.age`) are injected with a tiered scoping model:

1. **Per-skill env** -- If the skill declares its own `env` list in the Agentfile, only those variables are injected.
2. **Per-agent env** -- Otherwise, variables from the `[env]` section (`required` + `optional`) are injected.
3. **No Agentfile** -- Legacy mode: all vault variables are injected.

## Smart error messages

If you pass a registry-style image reference (e.g. `aide/github-reviewer`) instead of an instance name, the command suggests running `aide.sh pull` and `aide.sh run` first.

## Help output

Running `aide.sh exec <instance>` with no skill prints:
- Instance name, agent type, and version
- Each skill with its usage string and description
- Per-skill env var requirements
- A hint about semantic mode (`aide.sh exec -p <instance> "<query>"`)
