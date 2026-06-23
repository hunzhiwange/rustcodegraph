# CLI

Every RustCodeGraph command and the flags it accepts.

```bash
rustcodegraph                     # Run interactive installer
rustcodegraph install                 # Run installer (explicit)
rustcodegraph uninstall               # Remove RustCodeGraph from your agents (inverse of install)
rustcodegraph init [path]             # Initialize only; add -i/--index to build the graph too
rustcodegraph uninit [path]           # Remove RustCodeGraph from a project (--force to skip prompt)
rustcodegraph index [path]            # Full index (--force to re-index, --quiet for less output)
rustcodegraph sync [path]             # Incremental update
rustcodegraph status [path]           # Show statistics
rustcodegraph query <search>          # Search symbols (--kind, --limit, --json)
rustcodegraph files [path]            # Show file structure (--format, --filter, --max-depth, --json)
rustcodegraph context <task>          # Build context for AI (--format, --max-nodes)
rustcodegraph callers <symbol>        # Find what calls a function/method (--limit, --json)
rustcodegraph callees <symbol>        # Find what a function/method calls (--limit, --json)
rustcodegraph impact <symbol>         # Analyze what code is affected by changing a symbol (--depth, --json)
rustcodegraph affected [files...]     # Find test files affected by changes
rustcodegraph serve --mcp             # Start MCP server
```

## Query commands

`query`, `callers`, `callees`, and `impact` all accept `--json` for machine-readable output.

```bash
rustcodegraph query UserService --kind class --limit 10
rustcodegraph callers handleRequest --json
rustcodegraph impact AuthMiddleware --depth 3
```

## affected

Traces import dependencies transitively to find which test files are affected by changed source files. See [Affected Tests in CI](../guides/affected-tests.md) for options and a CI example.
