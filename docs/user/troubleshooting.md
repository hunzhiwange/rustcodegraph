# Troubleshooting

Fixes for the most common RustCodeGraph issues.

## "RustCodeGraph not initialized"

Run `rustcodegraph init -i` in your project directory first.

## Indexing is slow

Check that `node_modules` and other large directories are excluded (they are, if gitignored). Use `--quiet` to reduce output overhead.

## MCP hits `database is locked`

Current builds shouldn't: the Rust runtime uses SQLite in WAL mode, where concurrent reads normally do not block on a writer. If you still see it:

- **You're still running the old CodeGraph package or binary.** RustCodeGraph is a separate project and does not upgrade CodeGraph in place. Install RustCodeGraph separately — `curl -fsSL https://raw.githubusercontent.com/hunzhiwange/rustcodegraph/main/install.sh | sh` (macOS/Linux), `irm https://raw.githubusercontent.com/hunzhiwange/rustcodegraph/main/install.ps1 | iex` (Windows), or `npm i -g rustcodegraph` — then make sure your MCP config points at `rustcodegraph`.
- **`rustcodegraph status` shows `Journal:` other than `wal`** — WAL couldn't be enabled on this filesystem (common on network shares and WSL2 `/mnt`), so reads can block on writes. Move the project (with its `.rustcodegraph/` folder) onto a local disk.

## MCP server not connecting

Ensure the project is initialized/indexed, verify the path in your MCP config, and check that `rustcodegraph serve --mcp` works from the command line.

## Missing symbols

The MCP server auto-syncs on save (wait a couple of seconds). Run `rustcodegraph sync` manually if needed. Check that the file's language is [supported](./reference/languages.md) and isn't excluded by `.gitignore`.
