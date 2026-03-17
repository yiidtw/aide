# MCP Integration

MCP (Model Context Protocol) lets LLM hosts like Claude Code, Cursor, and Gemini call aide.sh agents as tools. Your agents become subagents that any LLM can orchestrate.

## What is MCP?

MCP is a standard protocol for LLMs to discover and invoke external tools. aide.sh implements an MCP server that exposes your running agents as tools.

## Auto-configure for Claude Code

```bash
$ aide.sh setup-mcp
Detected: Claude Code
Wrote MCP config to ~/.claude/settings.json
Registered tools: aide_list, aide_exec, aide_logs
```

This adds aide.sh as an MCP server in your Claude Code configuration.

## Manual setup

Add the following to your Claude Code `settings.json` or equivalent MCP config:

```json
{
  "mcpServers": {
    "aide": {
      "command": "aide-sh",
      "args": ["mcp-serve"]
    }
  }
}
```

For Cursor, add to `.cursor/mcp.json`:

```json
{
  "mcpServers": {
    "aide": {
      "command": "aide-sh",
      "args": ["mcp-serve"]
    }
  }
}
```

## Available MCP tools

Once configured, the LLM host sees these tools:

| Tool | Description |
|------|-------------|
| `aide_list` | List all running instances and their skills |
| `aide_exec` | Execute a skill on a running instance |
| `aide_logs` | Retrieve recent logs for an instance |

## Example: Claude Code calling an agent

After setup, Claude Code can use your agents directly:

```
User: "Check if I have any PRs to review"

Claude: I'll check your GitHub PRs.
        [calling aide_exec: instance="reviewer", skill="pr", args=["list"]]

        You have 2 PRs awaiting review:
        - Fix auth middleware: PR #42 by alice
        - Add pagination: PR #58 by bob
```

The LLM discovers available skills via `aide_list`, picks the right one, and calls `aide_exec`.

## Running the MCP server manually

```bash
$ aide-sh mcp-serve
MCP server listening on stdio
```

This is what `setup-mcp` configures to run automatically. You rarely need to invoke it directly.

## Debugging

Check that instances are running:

```bash
$ aide.sh ps
NAME      IMAGE                  STATUS
reviewer  github-reviewer:0.1.0  running
```

Test a skill works before expecting MCP to use it:

```bash
$ aide.sh exec reviewer pr list
```

If the skill works via `exec` but not via MCP, check the MCP server logs:

```bash
$ aide.sh logs --mcp
```
