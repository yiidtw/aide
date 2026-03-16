# Dispatch & Sync

The aide daemon's core job: maintain agent state consistency across machines.
Machines are means. Data integrity is the purpose.

## Machine roles

```toml
[machines.local]
host = "localhost"
role = "dev"
always_on = false

[machines.server]
host = "your-server"
role = "prod"
always_on = true
```

Every machine runs `aide daemon`. They coordinate via amem.
One machine is leader for always-on tasks. Others participate when online.

## Task dispatch

```toml
[dispatch]
email_poll    = { on = "server" }           # always-on = server
email_send    = { on = "any" }              # stateless = anywhere
skill_dev     = { on = "local" }            # IDE = local
deploy        = { on = "server" }           # prod = server
llm_inference = { on = "any", prefer = "server" }
```

Dispatch is declarative. aide.toml says WHO does WHAT.
The daemon reads rules and routes tasks accordingly.

## Sync: three types

### 1. Vault sync (credentials)

```toml
[sync.vault]
method  = "age+push"       # age encrypt → scp to targets
trigger = "on-change"      # inotify/fsevents watch
targets = ["server"]
```

MVP: `aide sync vault` — encrypts with age, scp to all targets.
Watch mode: fsevents detects change → auto push.

### 2. Skill sync (code)

```toml
[sync.skills]
method    = "git"
repo      = "your-org/aide-skills"    # public skills
auto_pull = true                     # server auto-pulls on change
```

Skills are code. Code belongs in git. Server pulls when main updates.
See [SKILLS.md](SKILLS.md) for public vs private distinction.

### 3. Memory sync (agent state)

```toml
[sync.memory]
method   = "amem"
conflict = "causal"
```

MVP: SQLite + Lamport clock + last-write-wins.
Two machines rarely modify the same record simultaneously
(dev on local, email on server — different write domains).

## Sync status

`aide sync status` shows:

```
machine    vault     skills    memory    last-seen
local      v3        abc123    ts:42     now
server     v3        abc123    ts:41     2s ago
```

Version vectors for vault, git SHA for skills, Lamport timestamp for memory.
