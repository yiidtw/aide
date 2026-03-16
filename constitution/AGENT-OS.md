# Agent OS

aide.sh is an operating system for agents. Not for processes — for trust.

## OS mapping

```
Traditional OS                    aide.sh (Agent OS)
──────────────────────────────────────────────────────
Kernel                            aide daemon (Rust)
Kernel memory                     amem (system state)
User memory                       CrossMem (user knowledge)
Process                           agent instance (jenny, henry)
Process scheduler                 task dispatcher (aide.toml)
Syscall interface                 aide.toml API + MCP
IPC                               email + MCP
File system                       vault + memory.db
Device drivers                    integrations (Gmail, GitHub, CF...)
cgroup / resource limit           apay (budget)
SELinux / AppArmor                agentguard (behavioral constraints)
Compiler toolchain                galv (build-time verification)
```

## Container runtime analogy

```
Docker daemon  = aide daemon (the runtime, always running)
Container      = agent instance (jenny, henry — workloads)
Image layer    = trust layer (galv → arun → amem → apay)
Dockerfile     = aide.toml (declarative agent definition)
Registry       = trust registry (verified agent profiles)
Compose        = aide compose (multi-agent orchestration)
Network        = email + MCP (agent-to-agent communication)
Volume         = amem (persistent memory)
Health check   = agentguard invariant check (behavioral, not ping)
Resource limit = apay budget ($$$ limit, not CPU limit)
```

**Docker isolates processes. aide isolates trust boundaries.**

## Two kinds of memory

```
amem (kernel space):                    CrossMem (user space):
  - agent machine assignments             - papers the user read
  - email processing state                - customer conversation history
  - vault version vector                  - project design decisions
  - task routing table                    - semantic knowledge graph

  Must be CORRECT                         Must be USEFUL
  → formal verification                  → feedback loop
  → causal/strong consistency            → eventual consistency
  → system breaks if wrong               → agent is dumber if wrong
```

## Deployment topology

```
/home/aide/                          # Linux user = aide (the runtime)
├── .config/aide.toml                # platform config
├── agents/                          # virtual agents (containers)
│   ├── jenny/
│   │   ├── persona.toml
│   │   ├── rules.toml
│   │   └── memory.db
│   └── henry/
│       └── ...
├── shared/
│   ├── contacts.db                  # cross-agent shared state
│   └── audit.log                    # platform-wide audit trail
└── credentials/
    └── vault.age                    # encrypted, loaded at daemon start
```

One Linux user (`aide`) = one daemon process = one trust boundary.
Agents are tenants inside that boundary, like containers inside Docker.

## Namespace isolation

Agent internals use their own namespace — independent of host tooling:

```
Host (external)              Agent kernel (internal)
──────────────────────────────────────────────────────
wonskill, npm, pip...        aide-skill (agent's skill layer)
Claude memory, Codex...      aide-mem (agent's memory layer)
~/.claude/, ~/.codex/...     ~/.aide/instances/<name>/memory/
```

**Rules:**
- `aide-skill`: the skill interface as seen by the agent. Whether the
  underlying implementation is wonskill, a Rust binary, or a shell script
  is an implementation detail. The agent only sees `aide-skill <name> <cmd>`.
- `aide-mem`: the memory interface as seen by the agent. Whether the host
  uses Claude's memory, Codex's memory, or flat files is irrelevant.
  The agent reads/writes `aide-mem` — the daemon handles bridging.
- `aide mount` bridges aide-mem → host memory (e.g. Claude's AGENTS.md).
  `aide unmount` removes the bridge. The agent's memory survives unmount.

**Why:** Without namespace isolation, agent state leaks into host tooling
and vice versa. If Claude's memory format changes, agents break. If
wonskill renames a command, agents break. The kernel boundary prevents this.

```
aide call jenny.ydwu cool scan
  → daemon resolves: cool → aide-skill cool → wonskill cool scan
  → result stored in: aide-mem (jenny's instance memory)
  → optionally bridged to: Claude memory via aide mount
```

The mapping (aide-skill → wonskill, aide-mem → Claude memory) is
configured per-machine in aide.toml, not hardcoded in agent code.
