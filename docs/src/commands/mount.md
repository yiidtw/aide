# aide.sh mount / unmount

Inject agent context into LLM coding tools.

## Usage

```
aide.sh mount <INSTANCE> <TARGET>
aide.sh unmount <INSTANCE> <TARGET>
```

**TARGET** is one of: `claude`, `codex`, `gemini`, `all`.

## What gets injected

The mount command gathers content from the instance directory and writes it as a single markdown document:

1. **Instance metadata** -- agent type, email, role, cron schedules (from `instance.toml`).
2. **Persona** -- contents of `persona.md`.
3. **Seed knowledge** -- all `.md` files under `seed/`.
4. **Memory** -- all `.md` files under `memory/`.

Each section is separated by a horizontal rule. The file is marked with `<!-- aide-mount -->` so it can be cleanly removed on unmount.

## Target: claude

Writes to `~/.claude/projects/<cwd-key>/memory/aide_<instance>.md`.

The CWD is encoded as a path key (slashes replaced with dashes). If a `MEMORY.md` index file exists in that directory, an entry is appended under an `## Aide Agents` section.

```bash
aide.sh mount jenny.ydwu claude
# -> ~/.claude/projects/-Users-ydwu-projects-myapp/memory/aide_jenny.ydwu.md
```

## Target: codex

Writes to `./AGENTS.md` in the current working directory.

If `AGENTS.md` already exists with non-aide content, the agent context is appended after a separator. On unmount, only the aide-marked section is removed.

## Target: gemini

Writes to `./GEMINI.md` in the current working directory.

Same append/remove behavior as the codex target.

## Target: all

Mounts (or unmounts) to all three targets at once.

## Examples

```bash
aide.sh mount jenny.ydwu claude
aide.sh mount jenny.ydwu all
aide.sh unmount jenny.ydwu codex
aide.sh unmount jenny.ydwu all
```

## Unmount behavior

- **claude**: Deletes `aide_<instance>.md` and removes the index entry from `MEMORY.md`.
- **codex**: Removes the aide-marked section from `AGENTS.md`. Deletes the file if no other content remains.
- **gemini**: Same as codex, targeting `GEMINI.md`.
