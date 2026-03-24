---- MODULE Gateway ----
\* Router (aide binary): the only coordinator in an aide swarm.
\* Three actions ONLY: Dispatch, Merge, Spawn.
\* Router NEVER executes tasks itself — it is NOT a skill launcher.
\* Router = specialized aide that assigns tasks, merges or creates aides.

EXTENDS Naturals, FiniteSets, Sequences

CONSTANTS
    MaxAgents,      \* upper bound on total agents
    MaxToken,       \* total token budget for the system
    TaskTypes,      \* set of task type strings
    SkillSet        \* set of all possible skills

VARIABLES
    agents,         \* set of agent records [id, skills, token, alive]
    tasks,          \* sequence of pending tasks (bounded)
    pool            \* Nat: unallocated token

vars == <<agents, tasks, pool>>

\* ── Type invariant ──
TypeOK ==
    /\ agents \subseteq [id: Nat, skills: SUBSET SkillSet,
                          token: 0..MaxToken, alive: BOOLEAN]
    /\ pool \in 0..MaxToken

\* ── Safety invariants ──

\* Helper: sum of tokens
RECURSIVE SumSet(_)
SumSet(S) ==
    IF S = {} THEN 0
    ELSE LET a == CHOOSE x \in S : TRUE
         IN a.token + SumSet(S \ {a})

SumTokens(A) == SumSet({a \in A : a.alive})

\* INV1: Router never executes (by construction — no Execute action)

\* INV2: Agent IDs are unique
UniqueIds ==
    \A a1, a2 \in agents :
        (a1 # a2) => (a1.id # a2.id)

\* INV3: Alive agent count bounded
BoundedAgents == Cardinality({a \in agents : a.alive}) <= MaxAgents

\* INV4: Token conservation — pool + agent tokens = MaxToken
\* Dispatch costs token from agent, Merge redistributes, Spawn takes from pool
TokenConserved ==
    pool + SumTokens(agents) = MaxToken

\* ── Initial state ──
Init ==
    /\ agents = {}
    /\ tasks = <<>>
    /\ pool = MaxToken

\* ── Actions ──

\* 1. DISPATCH: assign a task to an agent (does NOT spend token — agent spends on exec)
Dispatch ==
    /\ tasks # <<>>
    /\ \E a \in agents :
        /\ a.alive
        /\ a.token > 0
        /\ tasks' = Tail(tasks)
        /\ UNCHANGED <<agents, pool>>

\* 2. MERGE: combine two agents — token preserved within agent set
Merge ==
    /\ \E a, b \in agents :
        /\ a # b
        /\ a.alive
        /\ b.alive
        /\ LET merged == [a EXCEPT
                !.skills = a.skills \cup b.skills,
                !.token = a.token + b.token]
               dead_b == [b EXCEPT !.alive = FALSE, !.token = 0]
           IN
               /\ agents' = (agents \ {a, b}) \cup {merged, dead_b}
               /\ UNCHANGED <<tasks, pool>>

\* 3. SPAWN: create a new agent from pool
RECURSIVE MaxId(_)
MaxId(S) == IF S = {} THEN 0
            ELSE LET x == CHOOSE y \in S : TRUE
                 IN IF x.id > MaxId(S \ {x})
                    THEN x.id
                    ELSE MaxId(S \ {x})

Spawn ==
    /\ Cardinality({a \in agents : a.alive}) < MaxAgents
    /\ pool >= 10
    /\ \E skills \in SUBSET SkillSet :
        /\ skills # {}
        /\ LET newId == 1 + MaxId(agents)
               newAgent == [id |-> newId, skills |-> skills,
                           token |-> 10, alive |-> TRUE]
           IN
               /\ agents' = agents \cup {newAgent}
               /\ pool' = pool - 10
               /\ UNCHANGED <<tasks>>

\* Environment: task arrives (bounded for model checking)
TaskArrives ==
    /\ Len(tasks) < 2
    /\ \E tt \in TaskTypes :
        /\ tasks' = Append(tasks, tt)
        /\ UNCHANGED <<agents, pool>>

\* ── Next state relation ──
Next ==
    \/ Dispatch
    \/ Merge
    \/ Spawn
    \/ TaskArrives

\* ── Specification ──
Spec == Init /\ [][Next]_vars

====
