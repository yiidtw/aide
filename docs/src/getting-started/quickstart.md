# Quick Start

Create your first agent in 2 minutes.

## 1. Initialize an Aidefile

```bash
cd ~/projects/my-reviewer
aide init --persona "Code Reviewer"
# ✓ Created Aidefile in ~/projects/my-reviewer
```

This creates an `Aidefile` in the current directory. That's all it takes to turn a Claude project into an agent.

## 2. Edit the Aidefile

```toml
[persona]
name = "Code Reviewer"
style = "thorough, direct"

[budget]
tokens = "100k"
max_retries = 3

[trigger]
on = "manual"

[vault]
keys = ["GITHUB_TOKEN"]
```

## 3. Run a task

```bash
aide run . "Review the latest PR and leave comments"
# ▸ Running task in ~/projects/my-reviewer
#   agent: Code Reviewer
#   budget: 100000 tokens
# ✓ Task completed (18,432 tokens used)
```

aide calls `claude -p` with the task, injects vault secrets as env vars, and enforces the token budget.

## 4. Register for convenience

```bash
aide register . --name reviewer
aide run reviewer "Check all open PRs for security issues"
```

## 5. Automate with triggers

Change `[trigger]` in the Aidefile:

```toml
[trigger]
on = "issue"
```

Then start the daemon:

```bash
aide up
```

Now the agent wakes up whenever a GitHub Issue is opened with the matching label.

## What's next?

- [Concepts](./concepts.md) — understand agents, Aidefiles, the registry
- [Aidefile reference](../guide/aidefile.md) — full configuration guide
- [Vault & Secrets](../guide/vault.md) — encrypting and injecting secrets
