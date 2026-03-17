#!/usr/bin/env bash
# diff — show current branch diff
# usage: diff [base]
set -euo pipefail
source "$(dirname "$0")/_check_gh.sh"; check_gh

BASE="${1:-main}"
BRANCH=$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo "HEAD")

echo "=== Diff: ${BRANCH} vs ${BASE} ==="
echo ""

STAT=$(git diff --stat "${BASE}...${BRANCH}" 2>/dev/null || git diff --stat "${BASE}" 2>/dev/null)
if [ -z "$STAT" ]; then
  echo "No changes."
  exit 0
fi

echo "$STAT"
echo ""
echo "=== Changed files ==="
git diff --name-only "${BASE}...${BRANCH}" 2>/dev/null || git diff --name-only "${BASE}" 2>/dev/null
