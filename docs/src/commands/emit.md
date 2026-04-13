# aide emit-claude-agents

Auto-generate Claude Code subagent wrappers from the aide registry.

```bash
aide emit-claude-agents
aide emit-claude-agents -o .claude/agents
```

## What it does

For each registered aide agent, generates a `.claude/agents/<name>.md` file that Claude Code recognizes as a custom subagent. Each wrapper is a thin dispatch layer:

1. Runs `aide dispatch <agent> "<task>"`
2. Captures the issue reference
3. Runs `aide wait <issue-ref>`
4. Returns the bounded summary

This lets a frontier Claude Code session dispatch work to aide agents natively via the Task tool interface, while aide handles token isolation externally.

## Flags

| Flag | Default | Description |
|------|---------|-------------|
| `-o` / `--output` | `.claude/agents` | Output directory for generated wrappers |

## Generated wrapper format

```markdown
---
name: crossmem-rs
description: "Dispatch crossmem-rs work via aide. Runs in isolated token budget."
tools:
  - Bash
---

You are a thin dispatch wrapper for the `crossmem-rs` aide agent.

## Rules
- You ONLY run `aide dispatch` and `aide wait` commands
- Do NOT attempt to do the work yourself

## Workflow
1. Run: `aide dispatch crossmem-rs "{task}"`
2. Capture the issue reference
3. Run: `aide wait {issue_ref}`
4. Return the summary
```

## When to use

Run this after registering new agents or changing Aidefiles. The generated wrappers are gitignore-safe (they're derived, not hand-written) and can be committed to your coordinator repo (e.g. crossmem-hq).
