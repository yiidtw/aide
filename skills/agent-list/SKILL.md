---
name: agent-list
description: List running aide agent instances and their status. Use when the user asks about running agents, agent status, or wants to see what agents are active.
---

# List Agent Instances

The user wants to see running aide agent instances.

Use the Bash tool to run:

```
aide ps
```

Present the output in a clear format. If no instances are running, suggest creating one with `aide run <image>`.
