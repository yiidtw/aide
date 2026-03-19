---
name: agent-exec
description: Execute a skill on a running aide agent instance. Use when the user wants to run a command or skill on an existing agent.
---

# Execute Agent Skill

The user wants to execute a skill on an aide agent instance.

Use the Bash tool to run:

```
aide exec $ARGUMENTS
```

If the user didn't specify which instance or skill, first run `aide ps` to list instances, then run `aide exec <instance> --help` to show available skills for that instance.
