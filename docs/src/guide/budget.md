# Budget

aide enforces a token budget per task. When the budget runs out, the task stops — no surprise bills.

## Configuration

```toml
[budget]
tokens = "100k"
max_retries = 3
```

- **tokens** — maximum tokens for the entire task (all retries combined)
- **max_retries** — how many times `claude -p` can be invoked for a single task

## How it works

1. aide calls `claude -p` with `--output-format json`
2. Claude Code returns token usage in its JSON output
3. aide tracks accumulated tokens across invocations
4. If `accumulated >= limit` or `invocations > max_retries`, the task stops

## Saturating arithmetic

The budget tracker uses saturating arithmetic — token counts can never overflow `u64::MAX`. This is formally verified with [kani](https://model-checking.github.io/kani/).

## Token shorthand

| Input | Value |
|-------|-------|
| `"50k"` | 50,000 |
| `"100k"` | 100,000 |
| `"1m"` | 1,000,000 |
| `"200000"` | 200,000 |

Default: `"200k"` if not specified.
