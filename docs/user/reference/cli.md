# 命令行界面

每个 RustCodeGraph 命令及其接受的标志。

```bash
rustcodegraph                     # Run interactive installer
rustcodegraph install                 # Run installer (explicit)
rustcodegraph uninstall               # Remove RustCodeGraph from your agents (inverse of install)
rustcodegraph upgrade [version]       # Upgrade the installed RustCodeGraph binary
rustcodegraph init [path]             # Initialize only; add -i/--index to build the graph too
rustcodegraph uninit [path]           # Remove RustCodeGraph from a project (--force to skip prompt)
rustcodegraph index [path]            # Full index (--force to re-index, --quiet for less output)
rustcodegraph sync [path]             # Incremental update
rustcodegraph status [path]           # Show statistics
rustcodegraph query <search>          # Search symbols (--kind, --limit, --json)
rustcodegraph explore <query...>      # Explore relevant symbols, source, call paths, and blast radius
rustcodegraph node <name>             # Show a symbol's source plus caller/callee trail
rustcodegraph node --file <path>      # Read an indexed file (--offset, --limit, --symbols-only)
rustcodegraph files [path]            # Show file structure (--format, --filter, --max-depth, --json)
rustcodegraph callers <symbol>        # Find what calls a function/method (--limit, --json)
rustcodegraph callees <symbol>        # Find what a function/method calls (--limit, --json)
rustcodegraph impact <symbol>         # Analyze what code is affected by changing a symbol (--depth, --json)
rustcodegraph affected [files...]     # Find test files affected by changes
rustcodegraph unlock [path]           # Remove a stale index lock
rustcodegraph serve --mcp             # Start MCP server
```

## 查询命令

大多数代码理解问题优先使用 `explore`。它会围绕查询中的符号、文件名或自然语言问题返回相关源码、调用路径和影响范围。

```bash
rustcodegraph explore "mutateElement renderStaticScene"
rustcodegraph explore "how does login reach the session middleware?"
```

需要查看单个符号或文件时使用 `node`。符号模式返回定义源码和调用关系；文件模式按已索引源码读取，可用 `--offset` / `--limit` 缩小范围。

```bash
rustcodegraph node AuthMiddleware
rustcodegraph node --file app/auth.py --offset 80 --limit 120
rustcodegraph node --file app/auth.py --symbols-only
```

需要先找候选符号、列调用关系或估算影响时，再使用 `query`、`callers`、`callees` 和 `impact`。这些命令均接受 `--json` 进行机器可读输出。

```bash
rustcodegraph query UserService --kind class --limit 10
rustcodegraph callers handleRequest --json
rustcodegraph impact AuthMiddleware --depth 3
```

## 受影响测试

间接跟踪导入依赖项以查找哪些测试文件受到更改的源文件的影响。See [Affected Tests in CI](../guides/affected-tests.md) for options and a CI example.
