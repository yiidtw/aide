# Expose (Email / PWA)

Give your agents an address so anyone can talk to them — no terminal required.

## Overview

| Channel | Address | Status |
|---------|---------|--------|
| **Email** | `agent+user@aide.sh` | Planned |
| **PWA** | `app.aide.sh/instance` | Planned |

Both channels are platform-controlled — aide.sh manages the infra, not third-party APIs.

## Email Gateway (planned)

Every agent gets an email address:

```
jenny+ydwu@aide.sh
```

### How it works

1. Anyone sends an email to `jenny+ydwu@aide.sh`
2. Cloudflare Email Worker receives the message
3. Routes to agent: `aide.sh exec -p jenny.ydwu "<email body>"`
4. Agent runs matching skills
5. Reply sent back via Resend/SMTP

### Why email

- **Zero install** — everyone already has an email client
- **Mobile native** — works in any phone's mail app
- **We control it** — aide.sh domain, our routing, no third-party bot limits

## PWA (planned)

A chat interface at `app.aide.sh`:

```
app.aide.sh/jenny.ydwu → chat UI → WebSocket → aide.sh exec
```

### Features

- Real-time chat with your agents
- Works on iOS, Android, Desktop (Add to Home Screen)
- Push notifications via Service Worker
- No App Store required

## Self-hosted integrations

Power users can integrate their agents with any messaging platform using
the MCP server or direct `aide.sh exec` calls:

```bash
# Your own Telegram bot
your-telegram-bot → aide.sh exec -p jenny.ydwu "message"

# Your own Discord bot
your-discord-bot → aide.sh exec -p jenny.ydwu "message"

# Any webhook
curl -X POST your-server/agent -d "message" → aide.sh exec
```

The MCP server (`aide.sh mcp`) is the recommended integration point for
LLM-based callers. For simple message-in/message-out, shell out to `aide.sh exec`.
