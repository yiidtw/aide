#!/usr/bin/env bash
# pr — list or review pull requests
# usage: pr [list|view NUMBER|diff NUMBER]
set -euo pipefail
source "$(dirname "$0")/_check_gh.sh"; check_gh

CMD="${1:-list}"
shift 2>/dev/null || true

case "$CMD" in
  list)
    echo "=== Open Pull Requests ==="
    gh pr list --limit 10
    ;;
  view)
    NUM="${1:?usage: pr view NUMBER}"
    gh pr view "$NUM"
    ;;
  diff)
    NUM="${1:?usage: pr diff NUMBER}"
    echo "=== PR #${NUM} Diff ==="
    gh pr diff "$NUM" --stat
    ;;
  *)
    echo "usage: pr [list|view NUMBER|diff NUMBER]"
    exit 1
    ;;
esac
