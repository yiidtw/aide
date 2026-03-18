#!/usr/bin/env bash
# ship — sync, test, push
# usage: ship [branch]
set -euo pipefail
if [ -n "${AIDE_PROJECT_DIR:-}" ]; then cd "$AIDE_PROJECT_DIR"; fi

source "$(dirname "$0")/../skills/_check_gh.sh" 2>/dev/null || true

BRANCH="${1:-$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo main)}"
echo "=== Ship: ${BRANCH} ==="
echo ""

echo "Status:"
git status --short
echo ""

echo "Commits ahead of main:"
git log --oneline main.."$BRANCH" 2>/dev/null || echo "(on main)"
echo ""

if command -v gh &>/dev/null; then
  echo "Open PRs:"
  gh pr list --head "$BRANCH" 2>/dev/null || echo "  none"
fi
echo ""
echo "To ship, run:"
echo "  git push origin ${BRANCH}"
echo "  gh pr create --fill"
