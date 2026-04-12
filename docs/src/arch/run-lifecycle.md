# How aide run Works

Detailed walkthrough of what happens when you run `aide run reviewer "Review PR #42"`.

## 1. Resolution

aide resolves `"reviewer"` by checking:
1. The registry (`~/.aide/config.toml`) for a matching name
2. If not found, treats it as a filesystem path and checks for an Aidefile

## 2. Aidefile parsing

The Aidefile is parsed from TOML. All sections are optional except that `[persona].name` defaults to `"unnamed"`.

## 3. Vault decryption

If `[vault].keys` is set:
1. Run `age -d -i vault.key vault.age` in the agent directory
2. Parse the decrypted output as `export KEY='VALUE'` lines
3. Filter to only the keys listed in `[vault].keys`
4. Store as `Vec<(String, String)>` for later injection

## 4. on_spawn hooks

Each hook in `[hooks].on_spawn` is executed in order:
- Built-in hooks (like `inject-vault`) are handled internally
- Custom hooks are run as shell commands in the agent directory

## 5. Task loop

```
while budget.can_invoke():
    result = claude -p <task> --output-format json
    budget.record(result.tokens_used)
    if result.success:
        break
```

Key details:
- `claude -p` runs with vault secrets as env vars (via `Command::env()`)
- Token usage is extracted from the JSON output
- Budget uses saturating arithmetic (formally verified with kani)
- The loop stops when the task succeeds OR budget/retries are exhausted

## 6. on_complete hooks

Same as on_spawn — executed in order after the task loop finishes.

## 7. Memory compaction check

If `[memory].compact_after` is set:
1. Estimate total tokens in `memory/` (file bytes / 4)
2. If over threshold, run `claude -p` with a compaction prompt
3. This compaction invocation also counts against the budget

## Dispatch flow (aide-as-subagent)

When an agent needs to delegate work to another agent, the dispatch flow provides token isolation:

```
aide dispatch → gh issue create → spawn aide run-issue →
  runner::run (budgeted claude -p) → build_summary →
  gh issue comment (bounded summary) → gh issue close →
  aide wait picks up summary → returns to frontier
```

### Why token isolation matters

Without dispatch, a sub-task runs inside the calling agent's context window. A 50k-token sub-task expands the frontier, consuming budget and degrading the caller's reasoning quality. With dispatch, the sub-agent runs in its own `claude -p` invocation with an independent token budget. Only the bounded summary (controlled by `[output].max_summary_tokens` in the Aidefile) flows back to the caller.

### How it works

1. **`aide dispatch <agent> "<task>"`** creates a GitHub issue labeled for the target agent, then spawns a detached `aide run-issue` worker. The caller gets back an issue URL immediately.
2. **`aide run-issue`** picks up the issue, runs the agent's task loop (same as `aide run`), builds a summary conforming to the `[output].narrative_schema`, posts it as a closing comment, and closes the issue.
3. **`aide wait <issue-url>`** polls the issue until it closes, then extracts the bounded summary from the final comment and returns it to the calling agent's context.

This three-step handshake keeps each agent's context window independent while allowing structured results to flow back through the GitHub Issues transport.
