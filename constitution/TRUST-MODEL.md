# Trust Model — Four Layers

Each layer adds a guarantee. An agent runs only when all layers verify.

```
Build time:     galv   — code soundness (Galois connections)
Runtime:        arun   — behavior safety (TLA+ model checking)
Distributed:    amem   — memory coherence (distributed consensus)
Payment:        apay   — budget control (spending bounds)
```

## Layer details

| Layer | Verifies | Method | Status |
|-------|----------|--------|--------|
| galv | Skill code is type-sound | Galois connection adjunction | OOPSLA paper in progress |
| arun (AgentGuard) | Agent behavior is safe | TLA+ model checking, DFA deny, taint tracking | 223 tests, ASE 2026 target |
| amem | State is consistent across machines | Distributed consensus (causal) | Research phase |
| apay | Spending within budget per skill | Budget accounting | Future — may use example.com |

## How layers compose (Docker analogy)

```
Docker image layer = code packaging (each layer adds files)
aide trust layer   = trust verification (each layer adds guarantees)
```

```toml
# aide.toml — trust section
[trust]
galv = { verify = "types + soundness" }
arun = { invariants = "8 safety properties" }
amem = { coherence = "causal consistency" }
apay = { budget = "10 USD/day" }
```

## Key insight: "Paxos for semantics"

Traditional consensus (Paxos/Raft) solves value agreement.
Agent memory coherence requires semantic agreement — "upgrade kfa" vs
"downgrade kfa" is not a timestamp conflict, it's a semantic conflict.

## MVP workarounds

| Layer | Full solution | MVP workaround |
|-------|--------------|----------------|
| galv | Galois adjunction checker | Limit to TypeScript skills only |
| arun | TLA+ model-checked runtime | CLI tool, not daemon-integrated |
| amem | Formal causal consistency | SQLite + Lamport clock + LWW |
| apay | Enforced budget accounting | aide.toml budget + manual check |

## Existing research assets

| Repo | Location | Evidence |
|------|----------|----------|
| AgentGuard (arun) | your-server:~/claude_projects/agentguard | 937 LOC TLA+, 223 tests, 500K transitions |
| Galv | your-server:~/claude_projects/galois-tower-runtime | 1,028 tests |
