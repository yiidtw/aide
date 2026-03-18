#!/usr/bin/env bash
# sites — list all CF Pages projects
# usage: sites
set -euo pipefail
TOKEN="${CLOUDFLARE_API_TOKEN:?not set}"
ACCOUNT="${CLOUDFLARE_ACCOUNT_ID:?not set}"
curl -s -H "Authorization: Bearer $TOKEN" \
  "https://api.cloudflare.com/client/v4/accounts/$ACCOUNT/pages/projects" | python3 -c "
import sys,json
d=json.load(sys.stdin)
for p in d.get('result',[]):
    domains = ', '.join([d['name'] for d in p.get('domains',[])]) or p.get('subdomain','')
    print(f'{p[\"name\"]:25s} {domains}')"
