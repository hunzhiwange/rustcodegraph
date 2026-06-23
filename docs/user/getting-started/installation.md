# Installation

Install RustCodeGraph and configure your AI coding agents.

## 1. Run the installer

```bash
rustcodegraph install
```

The installer will:

- Ask which agent(s) to configure — auto-detecting installed ones from **Claude Code**, **Cursor**, **Codex CLI**, **opencode**, **Hermes Agent**, **Gemini CLI**, **Antigravity IDE**, and **Kiro**.
- Prompt to install `rustcodegraph` on your `PATH` (so agents can launch the MCP server).
- Ask whether configs apply to all your projects or just this one.
- Write each chosen agent's MCP server config plus an instructions file (e.g. `CLAUDE.md`, `.cursor/rules/rustcodegraph.mdc`, `~/.codex/AGENTS.md`).
- Set up auto-allow permissions when Claude Code is one of the targets.
- Initialize your current project (local installs only).

## Non-interactive (scripting / CI)

```bash
rustcodegraph install --yes                              # auto-detect agents, install global
rustcodegraph install --target=cursor,claude --yes       # explicit target list
rustcodegraph install --target=auto --location=local     # detected agents, project-local
rustcodegraph install --print-config codex               # print snippet, no file writes
```

| Flag | Values | Default |
|---|---|---|
| `--target` | `auto`, `all`, `none`, or csv (`claude,cursor,…`) | prompt |
| `--location` | `global`, `local` | prompt |
| `--yes` | (boolean) | prompt every step |
| `--no-permissions` | (boolean) skip Claude auto-allow list | permissions on |
| `--print-config <id>` | dump snippet for one agent and exit | — |

## 2. Restart your agent

Restart your agent (Claude Code / Cursor / Codex CLI / opencode / Hermes Agent / Gemini CLI / Antigravity IDE / Kiro) for the MCP server to load.

## 3. Initialize projects

```bash
cd your-project
rustcodegraph init -i
```

This builds the per-project knowledge graph index and wires up any project-local agent surfaces, so a single global `rustcodegraph install` works in every project you open.

## Supported platforms

Every release ships a self-contained native build for all three desktop OSes,
on both x64 and arm64. Use the standalone installer, Homebrew on macOS/Linux,
or `npm i -g rustcodegraph` if you prefer npm.

| Platform | Architectures | Install |
|---|---|---|
| Windows | x64, arm64 | PowerShell installer or npm |
| macOS | x64, arm64 | shell installer, Homebrew, or npm |
| Linux | x64, arm64 | shell installer, Homebrew, or npm |

## Uninstall

Changed your mind? One command removes RustCodeGraph from every agent it configured:

```bash
rustcodegraph uninstall
```

This reverses the installer — stripping RustCodeGraph's MCP server config, instructions, and permissions from each configured agent. Your project indexes (`.rustcodegraph/`) are left untouched; remove those per-project with `rustcodegraph uninit`. Use `--target` to remove from specific agents, or `--yes` to run non-interactively.
