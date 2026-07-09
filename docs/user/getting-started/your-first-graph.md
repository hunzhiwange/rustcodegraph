# 你的第一张图表

构建索引并对其运行第一个查询。

安装 RustCodeGraph 后，构建和探索图表需要三个命令。

## 索引项目

```bash
cd your-project
rustcodegraph init -i      # initialize + index in one step
```

`init` 创建`.rustcodegraph/` 目录； `-i`（或 `--index`）立即构建完整索引。 对于现有项目，您可以随时重新索引：

```bash
rustcodegraph index          # full index
rustcodegraph sync           # incremental update of changed files
```

## 检查它是否有效

```bash
rustcodegraph status
```

这会报告节点/边缘/文件计数、活动 SQLite 后端和日志模式 - 快速运行状况检查索引是否已准备好。

## 运行查询

```bash
rustcodegraph query UserService          # find symbols by name
rustcodegraph callers handleRequest      # what calls a function
rustcodegraph callees handleRequest      # what a function calls
rustcodegraph impact AuthMiddleware      # what a change would affect
rustcodegraph context "fix the login flow"   # build task-focused context
```

每个都接受 `--json` 作为机器可读的输出。请参阅完整的[命令行界面参考](../reference/cli.md)。

## 交给你的代理人

如果存在 `.rustcodegraph/` 目录并配置了代理（请参阅[安装](./installation.md)），您的代理将自动使用 [MCP 工具](../reference/mcp-server.md) — 无需额外步骤。
