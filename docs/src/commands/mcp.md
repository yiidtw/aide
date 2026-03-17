# aide.sh mcp / setup-mcp

MCP (Model Context Protocol) server for LLM tool integration.

## aide.sh mcp

Start the MCP stdio server.

```
aide.sh mcp
```

Reads newline-delimited JSON-RPC 2.0 messages from stdin and writes responses to stdout. This is not meant to be called directly -- it is invoked by an MCP-capable LLM client (e.g. Claude Code).

### Protocol

- **Transport**: stdio (stdin/stdout), newline-delimited JSON
- **Protocol version**: `2024-11-05`
- **Server info**: `aide.sh` v0.1.0

### Tool schemas

**aide_list** -- List all running agent instances and their available skills.

```json
{
  "name": "aide_list",
  "inputSchema": { "type": "object", "properties": {} }
}
```

Returns instance names, types, status, email, and per-instance skill list with type (script/prompt) and description.

**aide_exec** -- Execute a skill on an agent instance.

```json
{
  "name": "aide_exec",
  "inputSchema": {
    "type": "object",
    "properties": {
      "instance": { "type": "string" },
      "skill": { "type": "string" },
      "args": { "type": "string" }
    },
    "required": ["instance", "skill"]
  }
}
```

Runs the skill script, applies env scoping from the vault, logs the invocation, and returns stdout/stderr.

**aide_logs** -- Read recent logs from an agent instance.

```json
{
  "name": "aide_logs",
  "inputSchema": {
    "type": "object",
    "properties": {
      "instance": { "type": "string" },
      "lines": { "type": "number" }
    },
    "required": ["instance"]
  }
}
```

Returns the last N log lines (default 50).

## aide.sh setup-mcp

Auto-configure MCP integration for a target client.

```
aide.sh setup-mcp [TARGET]
```

`TARGET` defaults to `claude`. Writes the MCP server configuration so that the client can discover and call `aide.sh mcp` automatically.

For Claude Code, this updates `~/.claude/claude_desktop_config.json` (or equivalent) with the aide.sh MCP server entry.
