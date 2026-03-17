# Agentfile.toml Reference

The Agentfile is the manifest that defines an agent. It is always named `Agentfile.toml` and lives at the root of the agent directory.

## Complete example

```toml
[agent]
name = "jenny"
version = "0.1.0"
description = "NTU GIEE PhD student assistant — school work, email, course management"
author = "ydwu"

[persona]
file = "persona.md"

[skills.cool]
script = "skills/cool.sh"
description = "NTU COOL LMS scanning (courses, assignments, grades)"
usage = "cool [courses|assignments|grades|todos|summary|scan]"
schedule = "0 8 * * *"
env = ["NTU_COOL_TOKEN"]

[skills.email]
script = "skills/email.sh"
description = "Email triage (POP3/SMTP)"
usage = "email [check|unread|read N|search Q|send TO SUBJ BODY]"
schedule = "0 */4 * * *"
env = ["SMTP_USER", "SMTP_PASS", "POP3_USER", "POP3_PASS"]

[skills.chrome]
script = "skills/chrome.sh"
description = "Chrome browser automation"
usage = "chrome [open|screenshot|scrape URL]"

[seed]
dir = "seed/"

[env]
required = ["NTU_COOL_TOKEN"]
optional = ["SMTP_USER", "SMTP_PASS", "POP3_USER", "POP3_PASS"]

[soul]
prefer = "claude-sonnet"
min_params = 1
```

## Sections

### [agent]

| Field | Required | Description |
|-------|----------|-------------|
| name | yes | Agent name (lowercase, alphanumeric, hyphens) |
| version | yes | Semver string |
| description | yes | One-line summary |
| author | no | Author name or handle |

### [persona]

| Field | Required | Description |
|-------|----------|-------------|
| file | yes | Path to a markdown file describing the agent's identity |

The persona file is used by LLMs in semantic mode (`-p`). It has no effect in explicit mode.

### [skills.NAME]

Each skill is a TOML table under `[skills]`.

| Field | Required | Description |
|-------|----------|-------------|
| script | yes* | Path to shell script (mutually exclusive with `prompt`) |
| prompt | yes* | Path to prompt markdown file (mutually exclusive with `script`) |
| description | no | Shown in `aide.sh exec <instance>` skill list and `--help` |
| usage | no | Usage string for `--help` |
| schedule | no | Cron expression for periodic execution |
| env | no | List of env var names this skill needs |

### [seed]

| Field | Required | Description |
|-------|----------|-------------|
| dir | yes | Directory of static files bundled into the image |

Seed data is copied into the instance at `aide.sh run` time. Useful for config files, templates, or reference data.

### [env]

| Field | Required | Description |
|-------|----------|-------------|
| required | no | Env vars that must be present; `aide.sh run` will fail without them |
| optional | no | Env vars that are used if available |

### [soul]

Controls LLM behavior when the agent runs in semantic mode.

| Field | Required | Description |
|-------|----------|-------------|
| prefer | no | Preferred LLM model identifier |
| min_params | no | Minimum parameters before falling back to LLM reasoning |

## Validation

Run `aide.sh lint <dir>` to check an Agentfile for errors before building:

```bash
$ aide.sh lint school/
Agentfile.toml: OK
Skills: 3 found, all scripts exist
Env: NTU_COOL_TOKEN required
```
