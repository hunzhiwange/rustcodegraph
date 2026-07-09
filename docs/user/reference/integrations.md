# 集成

支持的代理和手动 MCP 设置。

交互式安装程序会自动检测并配置每个受支持的代理 - 连接 MCP 服务器并写入其说明文件。

## 支持的代理

- **Claude Code**
- **Cursor**
- **Codex CLI**
- **opencode**
- **Hermes Agent**
- **Gemini CLI**
- **Antigravity IDE**
- **Kiro**

运行 `rustcodegraph install` 并选择您的代理；有关非交互式标志，请参阅[安装](../getting-started/installation.md)。

## 手动设置

如果您想自己连接，请全局安装：

```bash
# macOS / Linux
curl -fsSL https://raw.githubusercontent.com/hunzhiwange/rustcodegraph/main/install.sh | sh
```

将 MCP 服务器添加到 `~/.claude.json`：

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

可以选择自动允许 `~/.claude/settings.json` 中的只读工具：

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

:::提示
光标使用错误的工作目录启动 MCP 子进程。安装程序通过注入 `--path` 参数来为您处理这个问题；如果您手动连接 Cursor，请显式传递项目路径。
:::
