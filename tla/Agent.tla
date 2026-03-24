---- MODULE Agent ----
\* Single aide agent state machine.
\* Each aide = independent `ANTHROPIC_API_KEY="" claude -p` process.
\* Agent NEVER dispatches, merges, or spawns. That's Router's job.
\* POST-TASK HOOK: after every Exec, agent MUST commit memory or skill update.

EXTENDS Integers, Sequences, FiniteSets

CONSTANTS
    Skills,         \* set of all skill names
    TaskTypes,      \* set of task type strings
    MaxMemory       \* max memory entries (bounded for model checking)

VARIABLES
    status,         \* {idle, executing, hooking, hibernating}
    occupation,     \* SUBSET Skills (immutable after init)
    memory,         \* Seq of memory entries (append-only)
    bandit,         \* function: strategy -> {count, total_reward}
    token,          \* Int (current budget, can go negative -> hibernate)
    logs,           \* Seq of log entries (append-only)
    hookPending     \* BOOLEAN: TRUE when post-task hook must run

vars == <<status, occupation, memory, bandit, token, logs, hookPending>>

\* ── Memory entry ──
MemEntry == [task_type: TaskTypes, strategy: SUBSET Skills,
             reward: {0, 1}, tick: Nat]

\* ── Type invariant ──
TypeOK ==
    /\ status \in {"idle", "executing", "hooking", "hibernating"}
    /\ occupation \subseteq Skills
    /\ Len(memory) <= MaxMemory
    /\ token \in -1..5
    /\ hookPending \in BOOLEAN

\* ══════════════════════════════════
\* SAFETY INVARIANTS
\* ══════════════════════════════════

\* INV1: Occupation never changes after creation
\* Enforced by: no action modifies occupation

\* INV2: Memory is append-only (prefix preserved)
\* Enforced by: only Append() in PostTaskHook

\* INV3: Agent never dispatches, merges, or spawns
\* Enforced by construction: no such actions exist

\* INV4: PostTaskHook is atomic with bandit update
\* Enforced by: PostTaskHook updates both in one step

\* INV5: No duplicate memory entry (Ko rule)
\* Checked in PostTaskHook pre-condition

\* INV6: Every log entry corresponds to an execution
LogComplete ==
    Len(logs) >= Len(memory)

\* INV7: Agent cannot return to idle without running hook
\* If hookPending, agent MUST be in "hooking" or "executing" — never "idle"
HookGuarantee ==
    hookPending => status \notin {"idle"}

\* ── Initial state ──
Init ==
    /\ status = "idle"
    /\ occupation \in SUBSET Skills \ {{}}  \* non-empty skill set
    /\ memory = <<>>
    /\ bandit = [s \in SUBSET Skills |-> [count |-> 0, total |-> 0]]
    /\ token \in 1..3
    /\ logs = <<>>
    /\ hookPending = FALSE

\* ══════════════════════════════════
\* ACTIONS
\* ══════════════════════════════════

\* 1. EXEC: execute a task (dispatched by Router)
\*    Sets hookPending = TRUE — agent MUST run PostTaskHook before going idle
Exec ==
    /\ status = "idle"
    /\ token > 0
    /\ hookPending = FALSE
    /\ \E skill \in occupation, tt \in TaskTypes :
        LET cost == 1
        IN
            /\ status' = "executing"
            /\ token' = token - cost
            /\ hookPending' = TRUE
            /\ logs' = Append(logs, [type |-> "exec", skill |-> skill,
                                     task_type |-> tt, tick |-> Len(logs)])
            /\ UNCHANGED <<occupation, memory, bandit>>

\* 2. POST_TASK_HOOK: mandatory after Exec — update memory + bandit (ATOMIC)
\*    This is the "post-task hook" that every aide MUST run.
PostTaskHook ==
    /\ status = "executing"
    /\ hookPending = TRUE
    /\ Len(memory) < MaxMemory
    /\ \E tt \in TaskTypes, strat \in SUBSET occupation, reward \in {0, 1} :
        \* Ko rule: no duplicate
        LET entry == [task_type |-> tt, strategy |-> strat,
                      reward |-> reward, tick |-> Len(logs)]
            isDuplicate == \E i \in 1..Len(memory) :
                /\ memory[i].task_type = tt
                /\ memory[i].strategy = strat
                /\ memory[i].reward = reward
        IN
            /\ ~isDuplicate
            /\ memory' = Append(memory, entry)
            /\ bandit' = [bandit EXCEPT
                ![strat].count = bandit[strat].count + 1,
                ![strat].total = bandit[strat].total + reward]
            /\ status' = "idle"
            /\ hookPending' = FALSE
            /\ logs' = Append(logs, [type |-> "hook", task_type |-> tt,
                                     reward |-> reward, tick |-> Len(logs)])
            /\ UNCHANGED <<occupation, token>>

\* 2b. SKIP_HOOK: duplicate detected (Ko rule), skip memory but still complete hook
SkipHook ==
    /\ status = "executing"
    /\ hookPending = TRUE
    /\ \E tt \in TaskTypes, strat \in SUBSET occupation, reward \in {0, 1} :
        LET isDuplicate == \E i \in 1..Len(memory) :
                /\ memory[i].task_type = tt
                /\ memory[i].strategy = strat
                /\ memory[i].reward = reward
        IN
            /\ isDuplicate
            /\ status' = "idle"
            /\ hookPending' = FALSE
            /\ UNCHANGED <<occupation, memory, bandit, token, logs>>

\* 3. HIBERNATE: token depleted
Hibernate ==
    /\ token < 0
    /\ status \in {"idle", "executing"}
    /\ status' = "hibernating"
    /\ UNCHANGED <<occupation, memory, bandit, token, logs, hookPending>>

\* 4. WAKE: token injected (by Router merge or external)
Wake ==
    /\ status = "hibernating"
    /\ token > 0
    /\ hookPending = FALSE  \* can only wake if hook was completed
    /\ status' = "idle"
    /\ UNCHANGED <<occupation, memory, bandit, token, logs, hookPending>>

\* ── Next state relation ──
Next ==
    \/ Exec
    \/ PostTaskHook
    \/ SkipHook
    \/ Hibernate
    \/ Wake

\* ── Specification ──
Spec == Init /\ [][Next]_vars

\* ══════════════════════════════════
\* PROPERTIES
\* ══════════════════════════════════

\* Safety: occupation never changes
OccupationFrozen == [][occupation' = occupation]_vars

\* Safety: memory only grows
MemoryOnlyGrows == [][Len(memory') >= Len(memory)]_vars

\* Safety: hookPending is always resolved before idle
\* (captured by HookGuarantee invariant)

\* Liveness: if executing with hook pending, eventually goes idle
\* (with fairness on PostTaskHook/SkipHook)
EventuallyHookCompletes ==
    (status = "executing" /\ hookPending) ~> (status = "idle" /\ ~hookPending)

====
