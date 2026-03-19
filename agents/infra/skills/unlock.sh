#!/usr/bin/env bash
# unlock — remove CF Access app for a domain
# usage: unlock <domain>
set -euo pipefail
TOKEN="${CLOUDFLARE_API_TOKEN:?not set}"
ACCOUNT="${CLOUDFLARE_ACCOUNT_ID:?not set}"
DOMAIN="${1:?Usage: unlock <domain>}"
API="https://api.cloudflare.com/client/v4/accounts/$ACCOUNT"

APP_ID=$(curl -s -H "Authorization: Bearer $TOKEN" "$API/access/apps" | \
  python3 -c "import sys,json; apps=json.load(sys.stdin).get('result') or []; matches=[a for a in apps if a.get('domain')=='$DOMAIN']; print(matches[0]['id'] if matches else '')")

if [ -z "$APP_ID" ]; then
  echo "No Access app found for $DOMAIN"
  exit 0
fi

echo "Removing Access app for $DOMAIN (id: $APP_ID)..."
RESULT=$(curl -s -X DELETE -H "Authorization: Bearer $TOKEN" "$API/access/apps/$APP_ID")
echo "$RESULT" | python3 -c "import sys,json; d=json.load(sys.stdin); print('unlocked' if d['success'] else d.get('errors','fail'))"
