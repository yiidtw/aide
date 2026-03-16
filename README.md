<p align="center">
  <b>aide.sh</b> — Deploy AI agents, just like Docker.
</p>

<p align="center">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="MIT License"></a>
</p>

## Install

```bash
curl -fsSL https://aide.sh/install | bash
```

## Quick Start

```bash
aide.sh pull devops/sre-bot            # Pull an agent image
aide.sh run devops/sre-bot             # Start an agent instance
aide.sh exec sre-bot check-uptime      # Run a skill inside the agent
aide.sh ps                             # List running agents
aide.sh rm sre-bot                     # Remove the instance
```

## Docker Comparison

If you know Docker, you know aide.sh.

| Task | aide.sh | Docker |
|---|---|---|
| Pull an image | `aide.sh pull user/agent` | `docker pull user/image` |
| Run an instance | `aide.sh run user/agent` | `docker run user/image` |
| Execute a command | `aide.sh exec bot skill` | `docker exec ctr cmd` |
| List instances | `aide.sh ps` | `docker ps` |
| Stop an instance | `aide.sh stop bot` | `docker stop ctr` |
| Remove an instance | `aide.sh rm bot` | `docker rm ctr` |
| View logs | `aide.sh logs bot` | `docker logs ctr` |
| Build an image | `aide.sh build .` | `docker build .` |
| Push to registry | `aide.sh push agent` | `docker push image` |
| Search registry | `aide.sh search query` | `docker search query` |

## Agentfile.toml

Define your agent in a single file:

```toml
[agent]
name = "sre-bot"
version = "0.1.0"
description = "DevOps SRE agent — uptime monitoring, incident response, log analysis"
author = "yourname"

[persona]
file = "persona.md"

[skills.check-uptime]
script = "skills/check-uptime.sh"
schedule = "*/5 * * * *"
env = ["MONITORING_API_KEY"]

[skills.incident-response]
script = "skills/incident-response.sh"
env = ["PAGERDUTY_TOKEN", "SLACK_WEBHOOK"]

[skills.log-analysis]
script = "skills/log-analysis.sh"
schedule = "0 */6 * * *"

[env]
required = ["MONITORING_API_KEY"]
optional = ["PAGERDUTY_TOKEN", "SLACK_WEBHOOK"]
```

## Vault

Store secrets encrypted at rest. Agents access them by name.

```bash
aide.sh vault set MONITORING_API_KEY=sk-...
aide.sh vault set SLACK_WEBHOOK=https://hooks.slack.com/...
aide.sh vault list
```

## Publish Your Agent

### 1. Create the agent directory and manifest

```bash
mkdir -p my-agent/skills
cat > my-agent/Agentfile.toml << 'EOF'
[agent]
name = "my-agent"
version = "0.1.0"
description = "A helpful assistant that summarizes web pages"
author = "yourname"

[persona]
file = "persona.md"

[skills.summarize]
script = "skills/summarize.sh"
env = ["API_KEY"]

[env]
required = ["API_KEY"]
EOF
```

### 2. Write the persona

```bash
cat > my-agent/persona.md << 'EOF'
You are a concise summarization assistant.
When given a URL or block of text, return a clear three-sentence summary.
EOF
```

### 3. Write skill scripts

```bash
cat > my-agent/skills/summarize.sh << 'EOF'
#!/usr/bin/env bash
set -euo pipefail
# $1 = URL or text to summarize
echo "Summarizing: $1"
EOF
chmod +x my-agent/skills/summarize.sh
```

### 4. Build the image

```bash
aide.sh build my-agent
```

The build step runs a leak scanner that rejects images containing hard-coded
secrets (API keys, tokens, passwords). If the scan fails, move the secret
into the vault instead.

### 5. Log in and push

```bash
aide.sh login
aide.sh push yourname/my-agent
```

Your agent is now live on [hub.aide.sh](https://hub.aide.sh) and anyone can
`aide.sh pull yourname/my-agent`.

> **Important:** Never embed secrets in `Agentfile.toml`, `persona.md`, or
> skill scripts. Use `aide.sh vault set KEY=value` to store credentials and
> reference them through the `env` field in your Agentfile.

## Links

- **Website**: [aide.sh](https://aide.sh)
- **Agent Registry**: [hub.aide.sh](https://hub.aide.sh)
- **Issues**: [GitHub Issues](https://github.com/aide-sh/aide/issues)

## License

MIT
