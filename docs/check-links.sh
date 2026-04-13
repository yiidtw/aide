#!/usr/bin/env bash
# docs/check-links.sh — crawl docs.aide.sh and report broken links + missing assets
# Usage: bash docs/check-links.sh [base_url]

set -euo pipefail

BASE="${1:-https://docs.aide.sh}"
FAIL=0
CHECKED=0
ERRORS=()

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
DIM='\033[2m'
BOLD='\033[1m'
RESET='\033[0m'

# Get all pages from SUMMARY.md
PAGES=()
while IFS= read -r line; do
  if [[ "$line" =~ \(\./(.*)\) ]]; then
    page="${BASH_REMATCH[1]}"
    # .md → strip extension (CF Pages serves without .html)
    page="${page%.md}"
    # README → index
    if [[ "$page" == "README" ]]; then
      page=""
    fi
    PAGES+=("$page")
  fi
done < "$(dirname "$0")/src/SUMMARY.md"

echo -e "${BOLD}Checking ${#PAGES[@]} pages on ${BASE}${RESET}"
echo "──────────────────────────────────────────────────"

for page in "${PAGES[@]}"; do
  url="${BASE}/${page}"
  # Follow redirects (CF Pages 308 strips .html)
  status=$(curl -sI -o /dev/null -w "%{http_code}" -L --max-time 10 "$url" 2>/dev/null || echo "000")
  CHECKED=$((CHECKED + 1))

  if [[ "$status" == "200" ]]; then
    echo -e "  ${GREEN}✓${RESET} ${DIM}${status}${RESET} /${page}"
  else
    echo -e "  ${RED}✗${RESET} ${RED}${status}${RESET} /${page}"
    ERRORS+=("${status} /${page}")
    FAIL=$((FAIL + 1))
  fi
done

# Check key static assets
echo ""
echo -e "${BOLD}Checking static assets${RESET}"
echo "──────────────────────────────────────────────────"
ASSETS=(
  "css/variables.css"
  "css/general.css"
  "css/chrome.css"
  "FontAwesome/css/font-awesome.css"
  "fonts/fonts.css"
  "highlight.css"
  "favicon.svg"
  "favicon.png"
)

for asset in "${ASSETS[@]}"; do
  url="${BASE}/${asset}"
  status=$(curl -sI -o /dev/null -w "%{http_code}" --max-time 10 "$url" 2>/dev/null || echo "000")
  CHECKED=$((CHECKED + 1))

  if [[ "$status" == "200" ]]; then
    echo -e "  ${GREEN}✓${RESET} ${DIM}${status}${RESET} /${asset}"
  else
    echo -e "  ${RED}✗${RESET} ${RED}${status}${RESET} /${asset}"
    ERRORS+=("${status} /${asset}")
    FAIL=$((FAIL + 1))
  fi
done

# Check aide.sh/docs/ redirect
echo ""
echo -e "${BOLD}Checking aide.sh redirects${RESET}"
echo "──────────────────────────────────────────────────"
REDIRECTS=(
  "https://aide.sh/docs/"
  "https://aide.sh/docs/guide/vault/"
  "https://aide.sh/docs/commands/init/"
  "https://aide.sh/commands/init/"
  "https://aide.sh/guide/vault/"
)

for rurl in "${REDIRECTS[@]}"; do
  # Follow redirects, check final status
  final=$(curl -sI -o /dev/null -w "%{http_code}" -L --max-time 10 "$rurl" 2>/dev/null || echo "000")
  location=$(curl -sI --max-time 10 "$rurl" 2>/dev/null | grep -i "^location:" | head -1 | tr -d '\r' || true)
  CHECKED=$((CHECKED + 1))

  if [[ "$final" == "200" ]]; then
    echo -e "  ${GREEN}✓${RESET} ${DIM}${final}${RESET} ${rurl} ${DIM}${location}${RESET}"
  else
    echo -e "  ${RED}✗${RESET} ${RED}${final}${RESET} ${rurl} ${DIM}${location}${RESET}"
    ERRORS+=("${final} ${rurl}")
    FAIL=$((FAIL + 1))
  fi
done

# Summary
echo ""
echo "──────────────────────────────────────────────────"
if [[ $FAIL -eq 0 ]]; then
  echo -e "${GREEN}${BOLD}✓ All ${CHECKED} checks passed${RESET}"
else
  echo -e "${RED}${BOLD}✗ ${FAIL}/${CHECKED} checks failed:${RESET}"
  for err in "${ERRORS[@]}"; do
    echo -e "  ${RED}${err}${RESET}"
  done
  exit 1
fi
