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
    types: [opened]
  issue_comment:
    types: [created]

concurrency:
  group: agent-${{ github.event.issue.number }}
  cancel-in-progress: false

jobs:
  respond:
    runs-on: ubuntu-latest
    # Only respond to allowed users, never to bot's own comments
    if: |
      (github.event_name == 'issue_comment' &&
       github.event.comment.user.login != 'github-actions[bot]' &&
       contains(fromJSON('["owner-username"]'), github.event.comment.user.login)) ||
      (github.event_name == 'issues' &&
       contains(fromJSON('["owner-username"]'), github.event.issue.user.login))
    steps:
      - uses: actions/checkout@v4

      - name: Install aide
        run: |
          VERSION="0.4.0"
          curl -sL -o "$HOME/.local/bin/aide" \
            "https://github.com/yiidtw/aide/releases/download/v${VERSION}/aide-$(uname -m)-unknown-linux-gnu"
          chmod +x "$HOME/.local/bin/aide"

      - name: Run agent
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
        run: |
          export PATH="$HOME/.local/bin:$PATH"
          if [ "${{ github.event_name }}" = "issue_comment" ]; then
            BODY="${{ github.event.comment.body }}"
          else
            BODY="${{ github.event.issue.body }}"
          fi
          ISSUE="${{ github.event.issue.number }}"

          RESULT=$(aide exec -p agent "$BODY" 2>&1 || echo "Agent error")
          gh issue comment "$ISSUE" --body "$RESULT"

      - name: Commit memory changes
        run: |
          git config user.name "agent[bot]"
          git config user.email "agent@aide.sh"
          git pull --rebase || true
          git add memory/ || true
          git diff --cached --quiet || git commit -m "memory: updated by agent (issue #${{ github.event.issue.number }})"
          git push || true
```

## Security Considerations

- **Access control**: The `if` condition restricts which users can trigger the agent. Replace `owner-username` with your GitHub username. Without this, anyone who can open issues triggers your agent.
- **Secrets**: API keys go in GitHub repo Settings → Secrets, not in memory files. Memory files should never contain credentials.
- **Self-trigger prevention**: The bot check (`github-actions[bot]`) prevents infinite comment loops.
- **Concurrency**: The `concurrency` group ensures only one run per issue at a time, preventing git push conflicts.
- **Version pinning**: aide is installed from a pinned release version, not `curl | bash`, to prevent supply-chain attacks.
- **Private repos**: GitHub Actions free tier gives 2000 minutes/month for private repos. Public repos are unlimited.
- **Memory encryption**: For sensitive data, encrypt memory files before committing (e.g., `age` encryption, same as aide vault).

## No Vendor Lock-in

The entire system is portable:
- **GitHub → GitLab**: Change webhook URL, same workflow syntax
- **GitHub → Gitea**: Self-hosted, same git structure
- **GitHub → bare git**: Use git hooks instead of Actions

The agent definition (Agentfile + skills + memory) is platform-agnostic. Only the webhook handler changes.

## Trade-offs

### What git-native is good at
- **Simplicity**: no database, no server, no custom UI to maintain
- **Portability**: move to any git host, same structure
- **Auditability**: every agent action is a git commit
- **Collaboration**: PRs to review agent behavior changes
- **Cost**: free for public repos on GitHub

### What git-native is NOT good at
- **Latency**: GitHub Actions cold-start is ~30-60 seconds. Not suitable for real-time chat.
- **Structured queries**: `git log` is not a database. Complex queries over memory are hard.
- **Scale**: thousands of concurrent issues would strain Actions runners.
- **Always-on**: agents wake on events, not continuously running. Use `aide up` daemon for cron/polling.

### When to use what

| Use case | Best approach |
|----------|-------------|
| Async tasks (email triage, reports) | Git-native (issues) |
| Real-time chat | `aide up` daemon + Telegram/email |
| CI/CD automation | Git-native (PR events) |
| Monitoring + alerting | `aide up` daemon (cron) |
| Interactive development | CLI (`aide exec`) or MCP |

## Comparison

| | aide.sh (git-native) | Paperclip | OpenClaw |
|---|---|---|---|
| Interface | GitHub Issues | Custom UI | Telegram/Slack |
| Memory | Git files | PostgreSQL | SQLite |
| Latency | ~30-60s (Actions) | Instant (server) | Instant (daemon) |
| Scale | Per-repo limits | Horizontal | Single machine |
| Orchestration | company.toml | Org charts + budgets | Single agent |
| Sync | git pull | DB replication | Manual |
| Migration | Change git remote | Re-deploy stack | Re-configure |
| Cost (public) | Free | Self-host infra | Self-host infra |
| Cost (private) | 2000 min/mo | Self-host infra | Self-host infra |
| Mobile | GitHub app | None built-in | Telegram app |
