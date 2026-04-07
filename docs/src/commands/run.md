# aide run

Execute a task in an agent's directory.

## Usage

```bash
aide run <agent> <task>
```

- **agent** — registered name or path to directory with Aidefile
- **task** — natural language task description

## Examples

```bash
# By registered name
aide run reviewer "Review PR #42"

# By path
aide run ./my-agent "Summarize recent changes"
aide run ~/projects/ops "Check server health"
```

## What happens

1. Resolve agent name → directory path
2. Load and parse Aidefile
3. Decrypt vault secrets (if configured)
4. Run `on_spawn` hooks
5. Loop: invoke `claude -p` with the task
   - Track token usage after each invocation
   - Stop if budget exhausted or task complete
6. Run `on_complete` hooks
7. Check memory compaction threshold

## Output

```
▸ Running task in ~/projects/code-reviewer
  agent: Senior Reviewer
  budget: 100000 tokens
✓ Task completed (23,847 tokens used)
```

Or on budget exhaustion:

```
✗ Task incomplete (100000 tokens used, budget exhausted)
```
