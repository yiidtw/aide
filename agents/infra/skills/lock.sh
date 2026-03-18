#!/usr/bin/env bash
# lock — lock a site with CF Access
# usage: lock <domain>
set -euo pipefail
TOKEN="${CLOUDFLARE_API_TOKEN:?not set}"
ACCOUNT="${CLOUDFLARE_ACCOUNT_ID:?not set}"
DOMAIN="${1:?Usage: lock <domain>}"

echo "Locking ${DOMAIN} with CF Access..."
RESULT=$(curl -s -X POST -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
  "https://api.cloudflare.com/client/v4/accounts/$ACCOUNT/access/apps" \
  -d "{\"name\":\"$DOMAIN\",\"domain\":\"$DOMAIN\",\"type\":\"self_hosted\",\"session_duration\":\"24h\"}")
echo "$RESULT" | python3 -c "import sys,json; d=json.load(sys.stdin); print('locked' if d['success'] else d.get('errors','fail'))"
