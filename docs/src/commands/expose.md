# aide.sh expose

Expose an agent instance via an external messaging channel.

## Usage

```
aide.sh expose <INSTANCE> <CHANNEL> [--token TOKEN]
```

## Options

| Flag | Description |
|------|-------------|
| `INSTANCE` | Agent instance name |
| `CHANNEL` | Messaging channel (currently: `telegram`) |
| `--token TOKEN` | Bot token for the channel |

## Telegram

Connects an agent instance to a Telegram bot. Incoming messages are routed to the agent's skills; responses are sent back to the chat.

```bash
aide.sh expose jenny.ydwu telegram --token "123456:ABC..."
aide.sh expose jenny.ydwu telegram    # reads TELEGRAM_BOT_TOKEN from vault
```

### Token resolution

1. If `--token` is provided on the command line, that value is used.
2. Otherwise, the command reads `TELEGRAM_BOT_TOKEN` from the encrypted vault (`~/.aide/vault.age`).
3. If neither source provides a token, the command fails with an error.

## Future channels

The `expose` command is designed to be extended with additional channels. The `channel` argument determines which adapter is used. Currently only `telegram` is implemented.

## Related commands

- `aide.sh vault set TELEGRAM_BOT_TOKEN=<value>` -- Store the bot token in the vault.
- `aide.sh exec <instance> <skill>` -- The same skill execution that `expose` delegates to.
