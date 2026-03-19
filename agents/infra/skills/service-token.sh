#!/usr/bin/env bash
# service-token — create a CF Access service token for machine-to-machine access
# usage: service-token <name>
# Outputs the Client-ID and Client-Secret (save these — secret is shown only once)
set -euo pipefail
TOKEN="${CLOUDFLARE_API_TOKEN:?not set}"
ACCOUNT="${CLOUDFLARE_ACCOUNT_ID:?not set}"
NAME="${1:?Usage: service-token <name>}"
API="https://api.cloudflare.com/client/v4/accounts/$ACCOUNT/access/service_tokens"

# Check if token with this name already exists
EXISTING=$(curl -s -H "Authorization: Bearer $TOKEN" "$API" | \
  python3 -c "import sys,json; tokens=json.load(sys.stdin).get('result') or []; matches=[t for t in tokens if t.get('name')=='$NAME']; print(matches[0]['id'] if matches else '')")

if [ -n "$EXISTING" ]; then
  echo "Service token '$NAME' already exists (id: $EXISTING)"
  echo "To recreate, delete it first in CF Dashboard or via API."
  exit 0
fi

echo "Creating service token: $NAME..."
RESULT=$(curl -s -X POST -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
  "$API" -d "{\"name\": \"$NAME\"}")

python3 -c "
import sys, json
d = json.load(sys.stdin)
if not d['success']:
    print('ERROR:', d.get('errors'))
    sys.exit(1)
r = d['result']
print(f'Service Token: {r[\"name\"]}')
print(f'  Client ID:     {r[\"client_id\"]}')
print(f'  Client Secret: {r[\"client_secret\"]}')
print()
print('Use these headers for machine access:')
print(f'  CF-Access-Client-Id: {r[\"client_id\"]}')
print(f'  CF-Access-Client-Secret: {r[\"client_secret\"]}')
print()
print('SAVE THE SECRET NOW — it will not be shown again.')
" <<< "$RESULT"
