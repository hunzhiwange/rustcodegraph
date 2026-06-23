# Integrations

Supported agents, and manual MCP setup.

The interactive installer auto-detects and configures each supported agent — wiring up the MCP server and writing its instructions file.

## Supported agents

- **Claude Code**
- **Cursor**
- **Codex CLI**
- **opencode**
- **Hermes Agent**
- **Gemini CLI**
- **Antigravity IDE**
- **Kiro**

Run `rustcodegraph install` and pick your agent(s); see [Installation](../getting-started/installation.md) for the non-interactive flags.

## Manual setup

If you'd rather wire it up yourself, install globally:

```bash
# macOS / Linux
curl -fsSL https://raw.githubusercontent.com/hunzhiwange/rustcodegraph/main/install.sh | sh
```

Add the MCP server to `~/.claude.json`:

```json
{
  "mcpServers": {
    "rustcodegraph": {
      "type": "stdio",
      "command": "rustcodegraph",
      "args": ["serve", "--mcp"]
    }
  }
}
```

Optionally auto-allow the read-only tools in `~/.claude/settings.json`:

```json
{
  "permissions": {
    "allow": [
      "mcp__rustcodegraph__rustcodegraph_search",
      "mcp__rustcodegraph__rustcodegraph_callers",
      "mcp__rustcodegraph__rustcodegraph_callees",
      "mcp__rustcodegraph__rustcodegraph_impact",
      "mcp__rustcodegraph__rustcodegraph_node",
      "mcp__rustcodegraph__rustcodegraph_status",
      "mcp__rustcodegraph__rustcodegraph_files"
    ]
  }
}
```

:::tip
Cursor launches MCP subprocesses with the wrong working directory. The installer handles this for you by injecting a `--path` argument; if you wire Cursor up by hand, pass the project path explicitly.
:::
