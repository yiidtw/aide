#!/usr/bin/env bash
# status — check all domains
# usage: status
set -euo pipefail
DOMAINS="aide.sh docs.aide.sh hub.aide.sh chatfounder.ai kfa.sh storylens.ai crossmem.dev cxfi.earth cxfi.org whatagentswant.ai"
for D in $DOMAINS; do
  CODE=$(curl -s -o /dev/null -w "%{http_code}" "https://$D/" 2>/dev/null || echo "ERR")
  printf "%-25s %s\n" "$D" "$CODE"
done
