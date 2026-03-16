# amem — Agent Memory Coherence

amem is the kernel memory layer of the Agent OS.
It guarantees that agent state is consistent across machines.

## What amem manages (kernel space)

```
- agent machine assignments
- email processing state (which messages handled, by whom)
- vault version vector
- task routing table
- skill execution history
- agent-to-agent coordination state
```

These are system-critical. If amem is wrong, the OS breaks:
two machines process the same email, vault diverges, tasks run twice.

## What amem does NOT manage (user space = CrossMem)

```
- papers the user read
- customer conversation history
- project design decisions
- semantic knowledge graph
```

User memory lives in CrossMem. It must be USEFUL, not necessarily CORRECT.
Eventual consistency is fine — agent is dumber if wrong, but system still runs.

## Consistency model

### The problem: "Paxos for semantics"

Traditional distributed consensus (Paxos/Raft) solves value agreement
on bytes. Agent memory requires semantic agreement:

```
Agent A says: "upgrade kfa to v2"
Agent B says: "downgrade kfa to v1"
```

This is not a timestamp conflict. It's a semantic conflict.
Last-write-wins destroys intent. Conflict resolution must be semantic-aware.

### Research direction

- Causal consistency as baseline (Lamport clocks for ordering)
- Semantic conflict detection (classify conflicts by domain)
- Policy-based resolution (aide.toml defines merge strategies per domain)
- Formal verification of consistency guarantees (TLA+ spec)

### Consistency levels (configurable per data type)

```toml
[sync.memory]
method   = "amem"
conflict = "causal"        # default

# Per-domain overrides
[sync.memory.overrides]
vault_version   = "strong"       # vault MUST be consistent
email_state     = "causal"       # ordering matters, not simultaneity
contacts        = "eventual"     # last-write-wins is fine
skill_cache     = "eventual"     # rebuild if stale
```

## MVP implementation

Full amem is a research project. MVP uses proven primitives:

```
Storage:    SQLite (one DB per machine)
Ordering:   Lamport clock (logical timestamps)
Conflict:   Last-write-wins (acceptable because write domains don't overlap)
Sync:       Pull-based (periodic + on-demand)
Transport:  SSH/SCP (reuse existing infra)
```

### Why LWW works for MVP

In practice, two machines rarely write the same record:
- Local Mac: skill development, code changes
- your-server: email polling, inference jobs

Different write domains → no real conflicts → LWW is safe.
When real conflicts appear, we'll have concrete cases to formalize.

## Data model

```sql
-- amem.db (on each machine)
CREATE TABLE amem_state (
    key       TEXT PRIMARY KEY,
    value     BLOB,
    version   INTEGER,          -- Lamport timestamp
    origin    TEXT,              -- which machine wrote this
    updated   TEXT               -- wall clock (human debugging only)
);

CREATE TABLE amem_log (
    seq       INTEGER PRIMARY KEY AUTOINCREMENT,
    key       TEXT,
    old_value BLOB,
    new_value BLOB,
    version   INTEGER,
    origin    TEXT,
    timestamp TEXT
);
```

## Sync protocol (MVP)

```
Machine A                          Machine B
─────────                          ─────────
aide sync memory ──────────────►
                                   compare version vectors
                                   send deltas where B > A
                  ◄──────────────  deltas
apply deltas (LWW)
                  ──────────────►  send deltas where A > B
                                   apply deltas (LWW)
```

Bidirectional pull. No leader election. Both machines converge.

## Path to formal model

1. MVP: SQLite + Lamport + LWW (now)
2. Collect real conflict cases from production usage
3. Classify conflicts by domain (vault, email, contacts, etc.)
4. Define semantic merge operators per domain
5. Write TLA+ spec for amem consistency protocol
6. Prove safety invariants (no lost updates, no duplicate processing)
7. Paper: "Semantic Consistency for Distributed Agent Memory"

## Relation to other layers

```
galv  → verifies skill code before execution
arun  → constrains agent behavior at runtime
amem  → ensures state consistency across machines  ← this doc
apay  → bounds spending per skill invocation
```

amem is the distributed systems layer. Without it, aide.sh only works
on one machine. With it, aide.sh becomes a true multi-machine agent OS.
