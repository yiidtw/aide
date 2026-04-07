# aide mcp

Start the MCP (Model Context Protocol) stdio server.

## Usage

```bash
aide mcp
```

This starts a JSON-RPC 2.0 server over stdin/stdout, allowing LLM hosts (Claude Code, Cursor, etc.) to use aide agents as tools.

## Available tools

| Tool | Description |
|------|-------------|
| `aide_run` | Run a task on a registered agent |
| `aide_list` | List all registered agents |
| `aide_spawn` | Create a new agent |
| `aide_vault_get` | Retrieve a secret from the vault |

## Claude Code integration

Add to your Claude Code MCP config (`.claude/settings.json`):

```json
{
  "mcpServers": {
    "aide": {
      "command": "aide",
      "args": ["mcp"]
    }
  }
}
```

Once configured, Claude Code can orchestrate your agents:

```
"Use the reviewer agent to check PR #42"
→ Claude calls aide_run(agent="reviewer", task="Review PR #42")
```

## Protocol

- Transport: stdio (line-delimited JSON)
- Methods: `initialize`, `tools/list`, `tools/call`
- Follows the [MCP specification](https://modelcontextprotocol.io/)
