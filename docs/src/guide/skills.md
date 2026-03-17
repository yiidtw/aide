# Skills

A skill is a named, executable capability of an agent. Skills are the primary unit of work in aide.sh.

## Script-based skills

The most common type. A skill backed by a shell script:

```toml
[skills.hello]
script = "skills/hello.sh"
description = "Greet someone"
usage = "hello [name]"
```

The script receives positional arguments via `$1`, `$2`, etc:

```bash
#!/usr/bin/env bash
# skills/hello.sh
NAME="${1:-world}"
echo "Hello, $NAME!"
```

```bash
$ aide.sh exec bot hello Alice
Hello, Alice!
```

## Prompt-based skills

A skill backed by a markdown prompt file, interpreted by an LLM at runtime:

```toml
[skills.summarize]
prompt = "skills/summarize.md"
description = "Summarize text using an LLM"
usage = "summarize <text>"
```

```markdown
<!-- skills/summarize.md -->
Summarize the following text in 3 bullet points:

{{input}}
```

Prompt skills always require an LLM. They are skipped if no LLM is configured.

## Execution model

When `aide.sh exec <instance> <skill> [args]` runs:

1. The instance working directory is set to the instance root (`~/.aide/instances/<name>/`)
2. Environment variables are injected in order: vault -> agent env -> skill env
3. The script runs as a subprocess with the scoped environment
4. stdout/stderr are captured and returned to the caller
5. Exit code is preserved (0 = success)

## Adding description and usage

The `description` and `usage` fields appear when listing skills:

```bash
$ aide.sh exec bot
Available skills:
  pr         GitHub PR management (list, review, approve, merge)
             Usage: pr [list|review|approve|merge|summary|diff]
  notifications  GitHub notifications triage
             Usage: notifications [check|unread|read N|search Q]
  chrome     Chrome browser automation
             Usage: chrome [open|screenshot|scrape URL]
```

## Example: skill with argument parsing

```bash
#!/usr/bin/env bash
# skills/pr.sh
set -euo pipefail

CMD="${1:-help}"

case "$CMD" in
  list)
    curl -s -H "Authorization: Bearer $GITHUB_TOKEN" \
      "https://api.github.com/repos/${2}/pulls" | jq '.[].title'
    ;;
  diff)
    curl -s -H "Authorization: Bearer $GITHUB_TOKEN" \
      "https://api.github.com/repos/${2}/pulls/${3}" | jq '.[]'
    ;;
  *)
    echo "Usage: pr [list REPO|diff REPO PR_NUM|review|approve|merge|summary]"
    exit 1
    ;;
esac
```

## Scheduled skills

Add a `schedule` field with a cron expression to run a skill periodically:

```toml
[skills.pr]
script = "skills/pr.sh"
schedule = "0 8 * * *"    # daily at 8 AM
```

Scheduled skills require the daemon (`aide.sh up`). See [Cron & Scheduling](./cron.md).
