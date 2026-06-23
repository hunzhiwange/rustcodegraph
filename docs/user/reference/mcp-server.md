# MCP Server

The tools RustCodeGraph exposes to AI agents over MCP.

RustCodeGraph runs as a [Model Context Protocol](https://modelcontextprotocol.io/) server. Start it with:

```bash
rustcodegraph serve --mcp
```

Agents configured by the installer launch this automatically. When a `.rustcodegraph/` index exists, the agent uses the tools below.

## Tools

| Tool | Purpose |
|---|---|
| `rustcodegraph_search` | Find symbols by name across the codebase |
| `rustcodegraph_callers` | Find what calls a function |
| `rustcodegraph_callees` | Find what a function calls |
| `rustcodegraph_impact` | Analyze what code is affected by changing a symbol |
| `rustcodegraph_node` | Get details about a specific symbol (optionally with source code) |
| `rustcodegraph_explore` | Return source for several related symbols grouped by file, plus a relationship map, in one call |
| `rustcodegraph_files` | Get the indexed file structure (faster than filesystem scanning) |
| `rustcodegraph_status` | Check index health and statistics |

## How agents should use it

RustCodeGraph *is* the pre-built search index. For "how does X work?", architecture, trace, or where-is-X questions, an agent should answer in a handful of RustCodeGraph calls and stop — typically with **zero file reads** — rather than re-deriving the answer with `grep` + `Read`. A direct RustCodeGraph answer is a handful of calls; a grep/read exploration is dozens.

The installer writes this guidance into each agent's instructions file automatically.
