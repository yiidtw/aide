# Skills — Public vs Private

Skills are what agents can do. They live in git repos and are synced
across machines via the skill sync mechanism.

## Taxonomy

```
Public skill:  something any user would expect
               → lives in public repo (your-org/aide-skills)
               → push to public = published

Private skill: personal, org-specific, or incubating
               → lives in private repo (your-org/aide or separate)
               → may graduate to public when mature
```

## Decision rule

```
Is this skill useful to a general aide.sh user?
  YES → public (aide-skills repo)
  NO  → private (aide repo or dedicated private repo)

Is this skill wrapping an existing tool (not reinventing)?
  YES → integrate, don't rebuild (e.g., gws for Gmail/Drive)
  NO  → build only if no alternative exists
```

## Current skill inventory

### Public skills (aide-skills repo, to be created)

| Skill | Description | Wraps |
|-------|-------------|-------|
| gws | Gmail, Drive, Calendar access | Google Workspace APIs |
| github | Issues, PRs, repos | GitHub API / gh CLI |
| cloudflare | DNS, email routing, analytics | CF API |
| resend | Outbound email | Resend API |
| telegram | Notifications, approval flow | TG Bot API |

### Private skills (aide repo)

| Skill | Description | Why private |
|-------|-------------|-------------|
| ntu-mail | NTU Exchange IMAP access | Org-specific |
| ntu-cool | NTU COOL LMS integration | Org-specific |
| ntu-adfs | NTU auth federation | Org-specific |
| vault-sync | Cross-machine credential sync | Infra-internal |
| deploy | GitOps deployment to your-server | Infra-internal |

### Incubating (private → may become public)

| Skill | Description | Graduation criteria |
|-------|-------------|-------------------|
| memory-api | Memory storage + retrieval | When amem API stabilizes |
| collab | LaTeX collaboration | When product launches |

## Skill structure

```
skills/
├── public/           # symlink or subtree from aide-skills
│   ├── gws/
│   │   ├── skill.toml
│   │   └── src/
│   └── github/
│       ├── skill.toml
│       └── src/
└── private/
    ├── ntu-mail/
    │   ├── skill.toml
    │   └── src/
    └── vault-sync/
        ├── skill.toml
        └── src/
```

## skill.toml format

```toml
[skill]
name = "gws"
version = "0.1.0"
description = "Google Workspace access — Gmail, Drive, Calendar"
public = true

[trust]
galv = { required = true }     # must pass type checking
arun = { scopes = ["email:read", "email:send"] }  # runtime permissions
apay = { max_cost_per_call = "0.01 USD" }          # budget per invocation

[dependencies]
credentials = ["AIDE_GOOGLE_CLIENT_ID", "AIDE_GOOGLE_CLIENT_SECRET", "AIDE_GMAIL_REFRESH_TOKEN"]
```
