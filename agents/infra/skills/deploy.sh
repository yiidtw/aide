#!/usr/bin/env bash
# deploy — deploy static site to CF Pages
# usage: deploy <dir> <project-name>
set -euo pipefail
DIR="${1:?Usage: deploy <dir> <project-name>}"
PROJECT="${2:?Usage: deploy <dir> <project-name>}"
export CLOUDFLARE_API_TOKEN="${CLOUDFLARE_API_TOKEN:?not set}"
wrangler pages deploy "$DIR" --project-name "$PROJECT" 2>&1 | tail -3
