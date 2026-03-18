# Architecture: Git-Native Agent Methodology

Every aide.sh agent is a git repo. Every interaction is a git operation.

## The Repo IS the Agent

```
my-agent/
├── Agentfile.toml          # manifest (like Dockerfile)
├── persona.md              # personality + behavior rules
├── skills/*.sh             # executable capabilities
├── memory/                 # persistent memory (git tracked)
│   ├── context.md          # current working context
│   ├── decisions.md        # past decisions + reasoning
│   └── contacts.md         # people/systems the agent knows
├── seed/                   # initial knowledge
└── .github/
    └── workflows/
        └── agent.yml       # webhook: issue → exec → comment
```

## GitHub Issues = Agent Inbox

```
You open an issue:        "check my grades this semester"
  → GitHub webhook fires
  → GitHub Action runs:   aide exec school cool grades
  → Action comments:      "Here are your grades: ..."
  → You get a notification
  → You reply:            "also check if any assignments are due"
  → Webhook fires again
  → Agent responds in comments
```

No custom UI. No polling. No KV limits. GitHub IS the interface.

## Why Git

| Concern | Git solution |
|---------|-------------|
| Memory persistence | Files in repo, versioned |
| Memory sync across machines | `git pull` |
| Audit trail | `git log` |
| Collaboration | Pull requests |
| Rollback | `git revert` |
| Branching experiments | `git branch` |
| Access control | Repo permissions |
| Notifications | GitHub notifications + email |
| Mobile access | GitHub mobile app |
| API | GitHub REST/GraphQL API |
| CI/CD | GitHub Actions (free for public repos) |
| Migration | Move to GitLab/Gitea, same structure |

## Three Interface Layers

```
Layer 1: CLI (local)
  aide exec school cool courses          # direct skill call
  aide exec -p school "what's due?"      # LLM-assisted

Layer 2: MCP (LLM-driven)
  Claude Code → aide_exec(school, cool)  # sub-agent mode

Layer 3: GitHub Issues (async, remote)
  Open issue → webhook → exec → comment  # event-driven
```

All three layers use the same agent, same skills, same memory. The interface changes, the agent doesn't.

## Memory is Just Files

No database. No KV store. No special API.

```markdown
<!-- memory/context.md -->
# Current Context

- Semester: 2026 Spring
- Courses: 4 (ML, SoCV, VLSI, Seminar)
- Next deadline: ML HW3 (2026-03-25)
- Last scan: 2026-03-18 07:00
```

Agent writes to memory files. Git tracks changes. `git log memory/` shows how the agent's understanding evolved over time.

## Webhook Handler Template

```yaml
# .github/workflows/agent.yml
name: Agent
on:
  issues:
    types: [opened, edited]
  issue_comment:
    types: [created]

jobs:
  respond:
    runs-on: ubuntu-latest
    if: github.event.comment.user.login != 'github-actions[bot]'
    steps:
      - uses: actions/checkout@v4

      - name: Install aide
        run: curl -fsSL https://aide.sh/install | bash

      - name: Run agent
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
        run: |
          export PATH="$HOME/.local/bin:$PATH"
          BODY="${{ github.event.comment.body || github.event.issue.body }}"
          ISSUE="${{ github.event.issue.number }}"

          # Run with -p for natural language
          RESULT=$(aide exec -p agent "$BODY" 2>&1 || echo "Agent error")

          # Comment back on the issue
          gh issue comment "$ISSUE" --body "$RESULT"

      - name: Commit memory changes
        run: |
          git add memory/ || true
          git diff --cached --quiet || git commit -m "memory: updated by agent"
          git push || true
```

## No Vendor Lock-in

The entire system is portable:
- **GitHub → GitLab**: Change webhook URL, same workflow syntax
- **GitHub → Gitea**: Self-hosted, same git structure
- **GitHub → bare git**: Use git hooks instead of Actions

The agent definition (Agentfile + skills + memory) is platform-agnostic. Only the webhook handler changes.

## Comparison

| | aide.sh (git-native) | Paperclip | OpenClaw |
|---|---|---|---|
| Interface | GitHub Issues | Custom UI | Telegram/Slack |
| Memory | Git files | PostgreSQL | SQLite |
| Sync | git pull | DB replication | Manual |
| History | git log | Query DB | Scroll chat |
| Migration | Change remote | Re-deploy | Re-configure |
| Cost | Free (public repos) | Self-host | Self-host |
| Mobile | GitHub app | None | Telegram app |
