# Infra Lessons Learned

## Auth
- NEVER use Firebase client-side auth gate for prototypes. It's bypassable (disable JS).
- USE Cloudflare Access for server-side auth. One policy locks any CF Pages site.
- CF Access handles Google/GitHub login without redirect URL configuration.
- The old research-deploy/research-firebase skills inject a `<div>` overlay — client-side only.
- Zero Trust team name: yiidtw (yiidtw.cloudflareaccess.com)
- Zero Trust plan: Free (50 users)
- aide-admin token has Access: Apps+Policies, Orgs+IdP+Groups, Service Tokens (Edit)
- Use `lock.sh` to create Access app + policy, `service-token.sh` for machine access.

## DNS
- Always delete old A records before adding CNAME (CF won't allow both).
- GoDaddy ghost MX records can block CF Email Routing — delete via API.
- chatfounder.ai had leftover GoDaddy A records pointing to 13.248.243.5.

## KV
- Don't use KV.list() in polling loops — 1000/day free limit.
- Use per-user KV keys (inbox:username) instead. get() has 100k/day limit.

## Deployment
- All sites on CF Pages, not Vercel (avoids deploy quota conflicts).
- aide-site, aide-docs, aide-hub, chatfounder-site are separate CF Pages projects.
- wrangler pages deploy for manual, GitHub Actions for auto.

## Domains
- aide.sh: landing page
- docs.aide.sh: mdbook
- hub.aide.sh: agent registry
- chatfounder.ai: AI startup team product
