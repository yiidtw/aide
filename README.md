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

## Links

- **Website**: [aide.sh](https://aide.sh)
- **Agent Registry**: [hub.aide.sh](https://hub.aide.sh)
- **Issues**: [GitHub Issues](https://github.com/aide-sh/aide/issues)

## License

MIT
