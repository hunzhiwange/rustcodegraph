# Your First Graph

Build an index and run your first queries against it.

Once RustCodeGraph is installed, building and exploring a graph takes three commands.

## Index a project

```bash
cd your-project
rustcodegraph init -i      # initialize + index in one step
```

`init` creates the `.rustcodegraph/` directory; `-i` (or `--index`) immediately builds the full index. For an existing project you can re-index any time:

```bash
rustcodegraph index          # full index
rustcodegraph sync           # incremental update of changed files
```

## Check it worked

```bash
rustcodegraph status
```

This reports the node/edge/file counts, the active SQLite backend, and the journal mode — a quick health check that the index is ready.

## Run a query

```bash
rustcodegraph query UserService          # find symbols by name
rustcodegraph callers handleRequest      # what calls a function
rustcodegraph callees handleRequest      # what a function calls
rustcodegraph impact AuthMiddleware      # what a change would affect
rustcodegraph context "fix the login flow"   # build task-focused context
```

Each accepts `--json` for machine-readable output. See the full [CLI reference](../reference/cli.md).

## Hand it to your agent

With a `.rustcodegraph/` directory present and an agent configured (see [Installation](./installation.md)), your agent uses the [MCP tools](../reference/mcp-server.md) automatically — no extra step.
