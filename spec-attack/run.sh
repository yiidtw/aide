#!/usr/bin/env bash
# ChatFounder-style spec attack on aide-gaia
# Find edge cases in: ACA routing, memory hooks, occupation/cognition, frontier model bypass
set -euo pipefail

export ANTHROPIC_API_KEY=""
OUTDIR="$(cd "$(dirname "$0")" && pwd)"

claude_call() {
  local out
  out=$(echo "$1" | timeout 180 claude -p --model sonnet 2>/dev/null) || out=""
  if [ -z "$out" ]; then
    sleep 5
    out=$(echo "$1" | timeout 180 claude -p --model sonnet 2>/dev/null) || out="ERROR"
  fi
  echo "$out"
}

TOPIC="An agent-call-agent (ACA) framework for GAIA benchmark using aide.sh.

Key mechanisms:
1. OCCUPATION/COGNITION: Each agent has a defined occupation (browser, coder, reader) with specific skills. A router agent classifies GAIA questions and dispatches to specialists via aide MCP (aide_exec).
2. MEMORY HOOKS: Agents persist state in git-native memory files. Memory hooks trigger on events (skill completion, error, new question) and update agent memory. Warm-start agents reuse memory from prior questions.
3. ACA ROUTING: A bandit mechanism selects which agent handles each question. Agents can propose sub-agents to call. The router uses aide.sh MCP tools (aide_list, aide_exec, aide_logs) to orchestrate.
4. FRONTIER MODEL BYPASS PROBLEM: The frontier LLM (Claude) that drives the router often bypasses aide entirely — instead of calling aide_exec to route to a specialist agent, it just answers the question directly using its own knowledge, or it reads the agent's skill script and re-implements the logic inline.
5. SKILL EXECUTION: Skills are shell scripts (web search, python exec, file parsing). They run in sandboxed aide instances with scoped env vars from vault.
6. CRITIC: After an agent answers, a critic agent challenges the answer. If the critic finds flaws, the question goes back to the agent (or a different agent).

The system must solve 165 GAIA validation questions (L1=53, L2=86, L3=26) where 38 have attached files. Questions require web browsing, code execution, document parsing, and multi-step reasoning."

K=5

mkdir -p "$OUTDIR/raw"

echo "=== Spec attack: aide-gaia ACA framework ==="

for run in $(seq 1 $K); do
  echo "  [Run $run/$K]..."
  result=$(claude_call "You are a senior QA engineer specializing in AI agent systems. Given this system spec:

\"$TOPIC\"

Find edge cases, failure modes, and tricky scenarios that would cause this system to fail on GAIA benchmark questions. Focus on:
- Cases where the ACA routing breaks or is suboptimal
- Cases where frontier models bypass aide.sh and go direct
- Memory hook race conditions or stale state
- Occupation/cognition mismatches (wrong agent for the job)
- Skill execution failures (timeouts, missing tools, sandbox limits)
- Critic agent failure modes
- Multi-step questions that require agent handoffs
- File-attached questions that need specific parsers

List exactly 15 edge cases, one per line, numbered. Be specific and concrete — each should be a distinct failure scenario with a plausible GAIA question type that triggers it.

Output ONLY the numbered list, nothing else.")
  echo "$result" > "$OUTDIR/raw/run${run}.txt"
  echo "    saved to raw/run${run}.txt"
  sleep 2
done

# Dedup across runs
echo "  [Dedup]..."
all_cases=""
for run in $(seq 1 $K); do
  all_cases+="
--- Run $run ---
$(cat "$OUTDIR/raw/run${run}.txt")
"
done

dedup_result=$(claude_call "Below are 5 independent lists of edge cases for an agent-call-agent GAIA benchmark system:

$all_cases

Your task:
1. Group edge cases that describe the SAME scenario (even if worded differently)
2. For each unique edge case, note which runs found it (e.g., runs 1,3,5)

Output as a numbered list in this EXACT format:
1. [short description] | runs: 1,2,3
2. [short description] | runs: 2,4
...

Be strict about grouping — only merge if they describe truly the same failure scenario.")
echo "$dedup_result" > "$OUTDIR/dedup.txt"

# Compute stats deterministically
echo "  [Stats]..."
python3 -c "
import re, sys

with open('$OUTDIR/dedup.txt') as f:
    lines = [l.strip() for l in f if re.match(r'^\d+\.', l.strip())]

S = len(lines)
freq = {}
for line in lines:
    m = re.search(r'runs?:\s*([\d,\s]+)', line)
    if m:
        runs = [x.strip() for x in m.group(1).split(',') if x.strip()]
        freq[len(runs)] = freq.get(len(runs), 0) + 1

f1 = freq.get(1, 0)
f2 = freq.get(2, 0)

if f2 > 0:
    N_est = S + (f1**2) / (2 * f2)
else:
    N_est = S + f1 * (f1 - 1) / 2

coverage = S / N_est if N_est > 0 else 1.0

print(f'S={S}')
print(f'f1={f1}')
print(f'f2={f2}')
print(f'N_est={N_est:.1f}')
print(f'coverage={coverage:.0%}')
print()
print(f'Unique edge cases found: {S}')
print(f'Singletons (rarest, most valuable): {f1}')
print(f'Estimated total edge cases: {N_est:.1f}')
print(f'Spec coverage: {coverage:.0%}')
" > "$OUTDIR/stats.txt"

cat "$OUTDIR/stats.txt"
echo ""
echo "=== DONE ==="
echo "Raw runs: $OUTDIR/raw/"
echo "Dedup:    $OUTDIR/dedup.txt"
echo "Stats:    $OUTDIR/stats.txt"
