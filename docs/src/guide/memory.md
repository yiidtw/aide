# Memory

Each agent has its own `memory/` directory — Claude Code's native memory system. aide adds auto-compaction.

## Configuration

```toml
[memory]
compact_after = "200k"
```

When the estimated token count of all files in `memory/` exceeds the threshold, aide triggers a compaction pass using `claude -p`.

## How compaction works

1. After each task, aide estimates the token count of `memory/` (rough heuristic: file size in bytes / 4)
2. If over threshold, aide runs `claude -p` with a compaction prompt
3. The compaction prompt asks Claude to consolidate and deduplicate memory files
4. The `commit-memory` hook (if configured) commits the result

## Per-agent isolation

Each agent's memory is scoped to its own `memory/` directory. Agents spawned with `aide spawn` get an empty `memory/` directory by default.

When exporting agents with `aide export`, the `memory/` directory is **not** included — each deployment starts fresh.

## Tips

- Set `compact_after` slightly below your typical task budget to avoid compaction eating into task tokens
- Use the `commit-memory` hook to track memory evolution in git history
- Memory compaction is a separate `claude -p` invocation and counts against the current budget
