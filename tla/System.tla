---- MODULE System ----
\* Complete aide swarm system — REFERENCE SPEC (not for TLC model checking).
\* Use Agent.tla and Gateway.tla separately for model checking.
\*
\* Two disjoint components:
\*   Router (aide binary): Dispatch, Merge, Spawn — NEVER Exec
\*   Agent (LLM worker):   Exec, PostTaskHook — NEVER Dispatch/Merge/Spawn
\*
\* Each aide = independent `ANTHROPIC_API_KEY="" claude -p` subprocess.
\* Router is a specialized aide that ONLY assigns tasks, merges or creates aides.
\* All aides have post-task hooks to update memory or skills.

EXTENDS Naturals, Sequences, FiniteSets

CONSTANTS
    MaxAgents,
    MaxToken,
    Skills,
    TaskTypes,
    MaxMemory,
    ConfidenceThreshold   \* the only human-tunable parameter (0..100)

VARIABLES
    \* ── Router state (aide binary) ──
    taskQueue,        \* Seq of tasks waiting (bounded)
    systemPool,       \* Nat: unallocated token
    banditState,      \* function: TaskType -> {attempts, successes}

    \* ── Agent state ──
    agents,           \* set of agent records
    \* Each agent: [id, skills, token, memory, status, hookPending]

    \* ── Signal board (ephemeral) ──
    signals           \* set of pending signals

vars == <<taskQueue, systemPool, banditState, agents, signals>>

\* ══════════════════════════════════
\* INVARIANTS (verified in Agent.tla + Gateway.tla separately)
\* ══════════════════════════════════

\* SYS1: systemPool + Σ agent.token = MaxToken (Gateway.tla: TokenConserved)
\* SYS2: Router never in agent set (by construction)
\* SYS3: Router has no Execute action (Gateway.tla: no Execute)
\* SYS4: Agent has no Dispatch/Merge/Spawn (Agent.tla: by construction)
\* SYS5: Agent skills immutable (Agent.tla: OccupationFrozen)
\* SYS6: Agent memory append-only (Agent.tla: MemoryOnlyGrows)
\* SYS7: Bounded agents (Gateway.tla: BoundedAgents)
\* SYS8: Post-task hook mandatory (Agent.tla: HookGuarantee)
\* SYS9: No duplicate memory (Agent.tla: Ko rule in PostTaskHook)

\* ══════════════════════════════════
\* ROUTER ACTIONS (aide binary, not agent)
\* ══════════════════════════════════

ReceiveTask ==
    /\ Len(taskQueue) < 2  \* bounded
    /\ \E tt \in TaskTypes :
        /\ taskQueue' = Append(taskQueue, tt)
        /\ UNCHANGED <<systemPool, banditState, agents, signals>>

Dispatch ==
    /\ taskQueue # <<>>
    /\ \E a \in agents :
        /\ a.status = "idle"
        /\ a.hookPending = FALSE
        /\ a.token > 0
        /\ taskQueue' = Tail(taskQueue)
        /\ UNCHANGED <<systemPool, banditState, agents, signals>>

Spawn ==
    /\ Cardinality({a \in agents : a.status # "dead"}) < MaxAgents
    /\ systemPool >= 10
    /\ \E skills \in SUBSET Skills :
        /\ skills # {}
        /\ LET newId == 1 + Cardinality(agents)
               newAgent == [id |-> newId, skills |-> skills,
                           token |-> 10, memory |-> <<>>,
                           status |-> "idle", hookPending |-> FALSE]
           IN
            /\ agents' = agents \cup {newAgent}
            /\ systemPool' = systemPool - 10
            /\ UNCHANGED <<taskQueue, banditState, signals>>

Merge ==
    /\ \E a, b \in agents :
        /\ a # b
        /\ a.status = "idle"
        /\ b.status = "idle"
        /\ a.hookPending = FALSE
        /\ b.hookPending = FALSE
        /\ LET merged == [a EXCEPT
                !.skills = a.skills \cup b.skills,
                !.token = a.token + b.token,
                !.memory = a.memory \o b.memory]
               dead == [b EXCEPT !.status = "dead", !.token = 0]
           IN
            /\ agents' = (agents \ {a, b}) \cup {merged, dead}
            /\ UNCHANGED <<taskQueue, systemPool, banditState, signals>>

\* ══════════════════════════════════
\* AGENT ACTIONS (worker, never dispatches/merges/spawns)
\* ══════════════════════════════════

AgentExec ==
    /\ \E a \in agents :
        /\ a.status = "idle"
        /\ a.hookPending = FALSE
        /\ a.token > 0
        /\ LET a2 == [a EXCEPT
                !.token = a.token - 1,
                !.status = "executing",
                !.hookPending = TRUE]
           IN
            /\ agents' = (agents \ {a}) \cup {a2}
            /\ UNCHANGED <<taskQueue, systemPool, banditState, signals>>

\* Post-task hook: MANDATORY after exec — updates memory + bandit
AgentPostTaskHook ==
    /\ \E a \in agents :
        /\ a.status = "executing"
        /\ a.hookPending = TRUE
        /\ Len(a.memory) < MaxMemory
        /\ \E tt \in TaskTypes, reward \in {0, 1} :
            LET entry == [task_type |-> tt, reward |-> reward]
                a2 == [a EXCEPT
                    !.memory = Append(a.memory, entry),
                    !.status = "idle",
                    !.hookPending = FALSE]
            IN
                /\ agents' = (agents \ {a}) \cup {a2}
                /\ banditState' = [banditState EXCEPT
                    ![tt].attempts = banditState[tt].attempts + 1,
                    ![tt].successes = banditState[tt].successes + reward]
                /\ UNCHANGED <<taskQueue, systemPool, signals>>

\* ══════════════════════════════════
\* SYSTEM
\* ══════════════════════════════════

Init ==
    /\ taskQueue = <<>>
    /\ systemPool = MaxToken
    /\ banditState = [tt \in TaskTypes |-> [attempts |-> 0, successes |-> 0]]
    /\ agents = {}
    /\ signals = {}

Next ==
    \/ ReceiveTask
    \/ Dispatch
    \/ Spawn
    \/ Merge
    \/ AgentExec
    \/ AgentPostTaskHook

Spec == Init /\ [][Next]_vars

====
