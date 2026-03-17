# Expose (Telegram / Email)

Expose an agent to external messaging platforms so users can interact with it outside the terminal.

## Telegram

### Setup

1. Create a bot via [@BotFather](https://t.me/BotFather) on Telegram
2. Copy the bot token
3. Expose your agent:

```bash
$ aide.sh expose jenny telegram --token $TG_TOKEN
Telegram bot @jenny_aide_bot connected.
Listening for messages...
```

### How it works

The Telegram gateway uses long polling (no webhook, no public IP needed):

1. User sends a message to the bot on Telegram
2. aide.sh receives the message via Telegram Bot API
3. The message is routed to the agent as a semantic exec (`-p`)
4. The LLM picks the right skill, executes it, and formats a reply
5. The reply is sent back to the Telegram chat

### Example interaction

```
User:     do I have new assignments?
Bot:      Checking COOL LMS...
          You have 2 new assignments:
          - VLSI Design HW3 (due Jun 15)
          - ML Lab Final Proposal (due Jun 20)
```

### Running in background

```bash
$ aide.sh expose jenny telegram --token $TG_TOKEN --daemon
Telegram bot started in background (PID 54321)
```

Or include it in the daemon:

```bash
$ aide.sh up
```

## Future gateways

These are planned but not yet implemented:

- **Email gateway** — agent receives email, processes it, and replies
- **PWA** — browser-based chat interface served by the dashboard

## Self-hosted vs platform modes

aide.sh expose runs entirely on your machine:

- No data leaves your network except Telegram API calls
- Secrets stay in the local vault
- The agent process runs locally

There is no hosted/cloud mode. All processing is local.
