# Agent Hub

Pull, run, done.

```bash
aide.sh pull aide/devops
aide.sh run aide/devops --name mybot
aide.sh exec mybot check-uptime
```

## Available Agents

| Name | Description | Source | License |
|------|-------------|--------|---------|
| **aide/code-review** | Plan, review, QA, ship | [garrytan/gstack](https://github.com/garrytan/gstack) | MIT |
| **aide/context** | API docs for coding agents | [andrewyng/context-hub](https://github.com/andrewyng/context-hub) | MIT |
| **aide/devops** | Uptime, incidents, log analysis | community | MIT |
| **aide/ntu-student** | University LMS + mail | [yiidtw/aide](https://github.com/yiidtw/aide) | MIT |
| **aide/weather** | Forecast + severe alerts | community | MIT |
| **aide/qa** | Test, regression, coverage | community | MIT |

## Publish Your Agent

```bash
aide.sh login                    # GitHub OAuth
aide.sh build my-agent/          # package
aide.sh push my-agent/           # publish to hub
```

Your agent will be available as `your-username/agent-name`.

## Build Your Own

```bash
aide.sh init my-agent            # scaffold
# edit Agentfile.toml + skills/
aide.sh lint my-agent/           # validate
aide.sh build my-agent/          # package
```

See [Agentfile.toml reference](./guide/agentfile.md) for the full spec.
