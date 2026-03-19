#!/usr/bin/env bash
# meet — run a three-sages debate
# usage: meet <topic> [--mode claude|triad] [--rounds N] [--budget N] [--termination plain|heyting]
set -euo pipefail

# ─── Parse args ───
TOPIC=""
MODE="triad"
MAX_ROUNDS=5
TOKEN_BUDGET=0  # 0 = unlimited
TERMINATION="plain"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --mode) MODE="$2"; shift 2 ;;
    --rounds) MAX_ROUNDS="$2"; shift 2 ;;
    --budget) TOKEN_BUDGET="$2"; shift 2 ;;
    --termination) TERMINATION="$2"; shift 2 ;;
    *) TOPIC="$TOPIC $1"; shift ;;
  esac
done
TOPIC=$(echo "$TOPIC" | xargs)

if [ -z "$TOPIC" ]; then
  echo "Usage: meet <topic> [--mode claude|triad] [--rounds N] [--budget N] [--termination plain|heyting]"
  echo ""
  echo "Modes:"
  echo "  claude  — all three sages played by Claude (Sonnet)"
  echo "  triad   — Plato=OpenAI, Socrates=Claude, Aristotle=Gemini"
  echo ""
  echo "Termination:"
  echo "  plain   — fixed round limit"
  echo "  heyting — stop on stable labelling (no reversals for 2 rounds)"
  exit 1
fi

# ─── State ───
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
STATE_DIR="$(dirname "$SCRIPT_DIR")/memory"
DEBATE_LOG="$STATE_DIR/last-debate.md"
TOKENS_USED=0
ROUND=0
PREV_VERDICT=""
STABLE_COUNT=0
CLAIM=""
ATTACK=""

mkdir -p "$STATE_DIR"

# ─── LLM dispatch ───
call_plato() {
  local prompt="$1"
  case "$MODE" in
    claude)
      echo "$prompt" | claude -p --model sonnet 2>/dev/null | tail -10
      ;;
    triad)
      local out=$(codex exec --skip-git-repo-check "$prompt" 2>&1)
      echo "$out" | tail -1
      ;;
  esac
}

call_socrates() {
  local prompt="$1"
  # Socrates is always Claude
  echo "$prompt" | claude -p --model sonnet 2>/dev/null | tail -10
}

call_aristotle() {
  local prompt="$1"
  case "$MODE" in
    claude)
      echo "$prompt" | claude -p --model sonnet 2>/dev/null | tail -10
      ;;
    triad)
      echo "$prompt" | gemini --sandbox=false 2>/dev/null
      ;;
  esac
}

# ─── Token estimation (rough: 1 token ≈ 4 chars) ───
estimate_tokens() {
  local text="$1"
  echo $(( ${#text} / 4 ))
}

check_budget() {
  if [ "$TOKEN_BUDGET" -gt 0 ] && [ "$TOKENS_USED" -ge "$TOKEN_BUDGET" ]; then
    echo "TOKEN_BUDGET_EXCEEDED"
    return 1
  fi
  return 0
}

# ─── Termination strategies ───
check_termination() {
  local verdict="$1"
  case "$TERMINATION" in
    plain)
      # Just check round limit (handled in main loop)
      return 1
      ;;
    heyting)
      # Converged if: IN, or stable verdict for 2 consecutive rounds
      if echo "$verdict" | grep -qi "^IN"; then
        echo "CONVERGED_IN"
        return 0
      fi
      if [ "$verdict" = "$PREV_VERDICT" ]; then
        STABLE_COUNT=$((STABLE_COUNT + 1))
      else
        STABLE_COUNT=0
      fi
      if [ "$STABLE_COUNT" -ge 1 ]; then  # 2 consecutive same = stable
        echo "STABLE_${verdict}"
        return 0
      fi
      PREV_VERDICT="$verdict"
      return 1
      ;;
  esac
}

# ─── Main debate loop ───
{
echo "═══════════════════════════════════════════════════════════"
echo "  THREE SAGES DEBATE"
echo "  Topic: $TOPIC"
echo "  Mode: $MODE | Rounds: $MAX_ROUNDS | Termination: $TERMINATION"
if [ "$TOKEN_BUDGET" -gt 0 ]; then
  echo "  Token budget: $TOKEN_BUDGET"
fi
echo "═══════════════════════════════════════════════════════════"
echo ""

VERDICTS=""

while [ $ROUND -lt $MAX_ROUNDS ]; do
  ROUND=$((ROUND + 1))
  echo "━━━ Round $ROUND/$MAX_ROUNDS ━━━"
  echo ""

  # ─── PLATO: propose/revise ───
  if [ $ROUND -eq 1 ]; then
    PLATO_PROMPT="You are Plato. Propose a universally quantified, falsifiable definition for: $TOPIC. Format: 'For all X, P(X) iff Q(X)'. Then explain in 2 sentences. Be bold and idealistic."
  else
    PLATO_PROMPT="You are Plato. Your previous definition was attacked:
Definition: $CLAIM
Counterexample: $ATTACK
Verdict: $VERDICT

Revise your definition to exclude this counterexample. You MUST keep the format 'For all X, P(X) iff Q(X)'. Explain your revision in 2 sentences."
  fi

  MODE_LABEL=$(echo "$MODE" | tr '[:lower:]' '[:upper:]')
  echo "[柏拉圖/Plato — $MODE_LABEL]"
  CLAIM=$(call_plato "$PLATO_PROMPT")
  TOKENS_USED=$((TOKENS_USED + $(estimate_tokens "$CLAIM")))
  echo "$CLAIM" | sed 's/^/  /'
  echo ""

  check_budget || break

  # ─── SOCRATES: attack ───
  SOCRATES_PROMPT="You are Socrates using the elenctic method. Attack this definition with ONE concrete counterexample. Be specific — name a real or vivid hypothetical person/situation. 2-3 sentences max.

Definition: $CLAIM"

  echo "[蘇格拉底/Socrates — Claude]"
  ATTACK=$(call_socrates "$SOCRATES_PROMPT")
  TOKENS_USED=$((TOKENS_USED + $(estimate_tokens "$ATTACK")))
  echo "$ATTACK" | sed 's/^/  /'
  echo ""

  check_budget || break

  # ─── ARISTOTLE: judge ───
  JUDGE_PROMPT="You are Aristotle, an empiricist judge. Given this debate round:

Definition: $CLAIM
Counterexample: $ATTACK

Your task: Label the definition's status as exactly one of:
  IN — definition withstands the counterexample
  OUT — definition is refuted by the counterexample
  UNDEC — counterexample is unclear or irrelevant

Reply with EXACTLY the label (IN, OUT, or UNDEC) on the first line, then one sentence of reasoning."

  echo "[亞里斯多德/Aristotle — $MODE_LABEL]"
  JUDGE_RAW=$(call_aristotle "$JUDGE_PROMPT")
  TOKENS_USED=$((TOKENS_USED + $(estimate_tokens "$JUDGE_RAW")))
  echo "$JUDGE_RAW" | sed 's/^/  /'
  echo ""

  # Extract verdict (first word that matches IN/OUT/UNDEC)
  VERDICT=$(echo "$JUDGE_RAW" | grep -oiE '^(IN|OUT|UNDEC)' | head -1 | tr '[:lower:]' '[:upper:]')
  [ -z "$VERDICT" ] && VERDICT="UNDEC"
  VERDICTS="$VERDICTS $VERDICT"

  echo "  ▸ Label: $VERDICT | Tokens: ~$TOKENS_USED"
  echo ""

  # ─── Check termination ───
  TERM_RESULT=""
  if TERM_RESULT=$(check_termination "$VERDICT"); then
    echo "══ $TERM_RESULT after round $ROUND ══"
    echo ""
    break
  fi

  check_budget || break
done

# ─── Summary ───
echo "═══════════════════════════════════════════════════════════"
echo "  DEBATE COMPLETE"
echo "  Topic: $TOPIC"
echo "  Mode: $MODE | Rounds: $ROUND/$MAX_ROUNDS | Termination: $TERMINATION"
echo "  Verdicts:$VERDICTS"
echo "  Tokens: ~$TOKENS_USED"
if [ -n "$TERM_RESULT" ]; then
  echo "  Result: $TERM_RESULT"
else
  echo "  Result: MAX_ROUNDS_REACHED"
fi
echo "═══════════════════════════════════════════════════════════"
} 2>&1 | tee "$DEBATE_LOG"
