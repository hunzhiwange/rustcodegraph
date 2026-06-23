# Get Started

Get up and running with RustCodeGraph in seconds.

Get up and running with RustCodeGraph in seconds.

## No Node.js required — one command grabs the right build for your OS

```bash
# macOS / Linux
curl -fsSL https://raw.githubusercontent.com/hunzhiwange/rustcodegraph/main/install.sh | sh

# Homebrew (macOS / Linux)
brew install hunzhiwange/tap/rustcodegraph

# Windows (PowerShell)
irm https://raw.githubusercontent.com/hunzhiwange/rustcodegraph/main/install.ps1 | iex
```

## Already have Node? Use npm instead (works on any version)

```bash
npm i -g rustcodegraph
```

RustCodeGraph ships native Rust binaries — nothing to compile, no Node runtime needed
after install. Then run `rustcodegraph install` to auto-configure your agent(s):
Claude Code, Cursor, Codex CLI, opencode, Hermes Agent, Gemini CLI,
Antigravity IDE, Kiro.

## Initialize Projects

```bash
cd your-project
rustcodegraph init -i
```

That's it — your agent will use RustCodeGraph tools automatically when a `.rustcodegraph/` directory exists.

Next: build [Your First Graph](./your-first-graph.md), or see the full [Installation](./installation.md) options.
