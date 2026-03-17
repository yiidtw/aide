# aide.sh init / lint

Scaffold and validate agent projects.

## aide.sh init

Generate a new agent project skeleton.

```
aide.sh init <NAME>
```

Creates a directory `<NAME>/` with:

```
<NAME>/
  Agentfile.toml      # pre-filled manifest with TODO placeholders
  persona.md           # starter persona template
  skills/hello.sh      # sample executable skill script
  seed/.gitkeep        # empty seed directory
```

The generated `Agentfile.toml` includes a complete example with `[agent]`, `[persona]`, `[skills.hello]`, `[seed]`, and `[env]` sections. The hello skill is already executable (`chmod 755`).

### Example

```bash
aide.sh init my-agent
cd my-agent
aide.sh lint              # validate the scaffold
aide.sh exec . hello      # run the sample skill
```

Fails if the directory already exists.

## aide.sh lint

Validate an Agentfile.toml and its referenced files.

```
aide.sh lint [PATH]
```

`PATH` defaults to the current directory.

### Checks performed

**Errors** (block build/push):

| # | Check |
|---|-------|
| 1 | `Agentfile.toml` parses as valid TOML |
| 2 | `agent.name` is non-empty |
| 3 | `agent.version` is non-empty |
| 4 | `agent.description` is present and not a TODO placeholder |
| 5 | `agent.author` is present and not a TODO placeholder |
| 6 | Each skill has either `script` or `prompt`, not both, not neither |
| 7 | Referenced script files exist |
| 8 | Script files are executable (`chmod +x`) |
| 9 | Referenced prompt files exist |
| 10 | Cron schedule expressions are valid (5-field format) |
| 11 | No credential leaks detected (scans for `sk-ant-`, `sk-proj-`, `AKIA`, `ghp_`, `gho_`, `eyJhbG`, `-----BEGIN`) |

**Warnings** (informational):

| # | Check |
|---|-------|
| 12 | Skill missing `description` |
| 13 | Skill missing `usage` |
| 14 | Seed directory declared but not found |
| 15 | Skills reference env vars but no `[env]` section exists |
| 16 | Per-skill env var not listed in `[env].required` or `[env].optional` |

### Example output

```
$ aide.sh lint
[pass] Agentfile.toml parsed
[pass] agent.name = "jenny"
[pass] agent.version = "0.1.0"
[pass] agent.description present
[pass] agent.author present
[pass] skills/cool.sh exists (executable)
[warn] skills.chrome: missing usage
[fail] skills/draft.sh: not executable
1 warning(s), 1 error(s)
```
