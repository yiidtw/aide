#!/usr/bin/env bash
# ci — check CI/Actions status
# usage: ci [branch]
set -euo pipefail
source "$(dirname "$0")/_check_gh.sh"; check_gh

BRANCH="${1:-$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo main)}"

echo "=== CI Status: ${BRANCH} ==="
gh run list --branch "$BRANCH" --limit 5
