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
