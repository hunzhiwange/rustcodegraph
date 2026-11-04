<div align="center">

# RustCodeGraph

English · [简体中文](README.zh-CN.md)

Semantic code intelligence for Claude Code, Cursor, Codex CLI, opencode, Hermes Agent, Gemini, Antigravity, and Kiro.

**Local-first · Rust native · MCP-ready**

[Documentation](docs/user/README.md) · [Issues](https://github.com/hunzhiwange/rustcodegraph/issues)

[![npm version](https://img.shields.io/npm/v/rustcodegraph.svg)](https://www.npmjs.com/package/rustcodegraph)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust native](https://img.shields.io/badge/Rust-native-brightgreen.svg)](https://github.com/hunzhiwange/rustcodegraph/releases)

</div>

## What It Is

RustCodeGraph indexes a project into a local `.rustcodegraph/` knowledge graph. It parses source with tree-sitter, stores symbols, files, calls, imports, framework routes, and cross-file relationships in SQLite, then exposes that graph to coding agents through MCP and a CLI.

Instead of asking an agent to repeatedly grep and read files, RustCodeGraph lets it ask structural questions directly:

- Where is this symbol defined?
- Who calls this function?
- What does this code call?
- What might change if I edit this symbol?
- How does a request, callback, render path, or bridge flow across files?

RustCodeGraph is the Rust port of the original TypeScript CodeGraph implementation, rebuilt around a native CLI, local SQLite index, and MCP workflow.

## Quick Start

### 1. Install

```bash
# macOS / Linux
curl -fsSL https://raw.githubusercontent.com/hunzhiwange/rustcodegraph/main/install.sh | sh

# Homebrew (macOS / Linux)
brew install hunzhiwange/tap/rustcodegraph

# Windows (PowerShell)
irm https://raw.githubusercontent.com/hunzhiwange/rustcodegraph/main/install.ps1 | iex

# npm, if you already use Node.js
npm i -g rustcodegraph
```

Open a new terminal after installing so `rustcodegraph` is on your `PATH`.

### 2. Connect Your Agent

```bash
rustcodegraph install
```

The installer auto-detects supported agents and writes the MCP configuration they need to launch RustCodeGraph.

Optional for Codex users: add the bundled [RustCodeGraph skill](skills/rustcodegraph/SKILL.md) and keep `rustcodegraph watch --path <project-root>` running during active development so Codex is prompted to use fresh indexed context for code search, navigation, flow tracing, and impact analysis.

### 3. Index a Project

```bash
cd your-project
rustcodegraph init -i
```

This creates `.rustcodegraph/`, builds the first index, and enables automatic sync while the MCP server is running.

### 4. Uninstall

```bash
rustcodegraph uninstall  # remove agent configuration
rustcodegraph uninit     # remove the current project's index
```

## Core Features

| Feature | Summary |
|---|---|
| Local knowledge graph | Stores code structure in project-local SQLite; no external indexing service required. |
| Agent-first context | `rustcodegraph_explore` returns relevant source, relationships, and flow paths in one response. |
| Search and navigation | Find symbols, callers, callees, and impact radius from CLI or MCP. |
| Auto-sync | Native file watchers keep the graph fresh after edits. |
| Framework awareness | Links routes, components, framework conventions, and bridge boundaries to the code that handles them. See [Supported Frameworks and Bridges](#supported-frameworks-and-bridges). |
| Cross-language bridges | Connects common Swift/Objective-C, React Native, Expo Modules, and native-view flows that plain static parsing usually misses. |
| Privacy-first | Code, paths, filenames, symbol names, and queries stay on your machine. |

## CLI

```bash
rustcodegraph install              # configure supported agents
rustcodegraph uninstall            # remove RustCodeGraph from agent configs
rustcodegraph init -i              # initialize and index the current project
rustcodegraph uninit               # remove the current project's index
rustcodegraph index                # rebuild the full index
rustcodegraph sync                 # run an incremental update
rustcodegraph status               # show index status
rustcodegraph query UserService    # search symbols
rustcodegraph explore "auth login" # return related source and flow paths
rustcodegraph node UserService     # show one symbol or file
rustcodegraph callers login        # show call sites
rustcodegraph callees login        # show outgoing calls
rustcodegraph impact login         # show affected code
rustcodegraph affected --stdin     # map changed files to affected tests
rustcodegraph upgrade              # update the installed binary
```

## MCP Tools

RustCodeGraph exposes a small MCP tool set optimized for coding agents:

| Tool | Purpose |
|---|---|
| `rustcodegraph_explore` | Primary tool for "how does X work?" and "how does X reach Y?" questions. |
| `rustcodegraph_node` | Full source for one symbol, overload set, or file, with caller/callee context. |
| `rustcodegraph_search` | Symbol search by name. |
| `rustcodegraph_callers` | All known call sites, including callback registrations. |

If a workspace has no `.rustcodegraph/` index, the MCP server reports itself inactive and hides its tools. Indexing always remains a user choice.

## Supported Agents

`rustcodegraph install` can configure:

- Claude Code
- Cursor
- Codex CLI
- opencode
- Hermes Agent
- Gemini CLI
- Antigravity IDE
- Kiro

## Supported Languages

**Source languages:** TypeScript/TSX, JavaScript/JSX, Python, Go, Rust, Java, C, C++, C#, PHP, Ruby, Swift, Kotlin, Dart, Pascal/Delphi, Scala, Lua, Luau, Objective-C, R, Nix, Terraform/OpenTofu, Solidity, Erlang, VB.NET, COBOL, CFML, and ArkTS.

**ArkTS (`.ets`):** Search structs, methods, and builder functions; follow local imports, static component calls, and heuristic links from direct reactive-field writes to component rendering. Incremental sync preserves these links. Dynamic receivers, external HarmonyOS APIs, and full ArkUI runtime semantics are outside this coverage. See the [ArkTS coverage record](docs/design/dynamic-dispatch-coverage-playbook.md#arkts-static-coverage) for limits and validation status.

**CFML (`.cfm`, `.cfc`, `.cfml`):** Search tag and script component/function declarations; follow unambiguous static component construction, inheritance, relative template includes, and local calls, including methods directly chained onto component construction. Incremental sync preserves these links. Runtime mappings, dynamic receiver inference, and embedded SQL analysis are outside this coverage. See the [CFML coverage record](docs/design/dynamic-dispatch-coverage-playbook.md#cfml-static-coverage) for grammar and validation limits.

**COBOL (`.cbl`, `.cob`, `.cpy`):** Search programs, sections, and paragraphs; follow unambiguous static PERFORM targets, literal program CALLs, and indexed COPY references. Incremental sync preserves cross-file links. Dynamic CALL targets, copybook expansion, and full compiler semantics are outside this coverage. See the [COBOL coverage record](docs/design/dynamic-dispatch-coverage-playbook.md#cobol-static-coverage) for grammar and validation limits.

**VB.NET (`.vb`):** Search namespaces, classes, modules, methods, properties, and events; follow unambiguous static calls and indexed namespace imports, including aliases and case-insensitive names. Incremental sync preserves cross-file references. Full compiler semantics, runtime event dispatch, and ambiguous overload resolution are outside this coverage. See the [VB.NET coverage record](docs/design/dynamic-dispatch-coverage-playbook.md#vbnet-static-coverage) for grammar and validation limits.

**Erlang (`.erl`, `.hrl`, `.escript`):** Search modules, functions distinguished by arity, and macros; follow static calls, indexed module imports, and local header references. Behaviour callback dispatch has a heuristic bridge when the indexed contract is unambiguous. Incremental sync preserves cross-file references. Full preprocessing, OTP runtime dispatch, and dynamic call evaluation are outside this coverage. See the [Erlang coverage record](docs/design/dynamic-dispatch-coverage-playbook.md#erlang-static-coverage) for validation limits.

**Solidity (`.sol`):** Search contracts, interfaces, libraries, structs, events, modifiers, and functions; follow unambiguous static calls, indexed imports, and inherited bare member calls. Incremental sync preserves cross-file references. Runtime dispatch, ambiguous overloads, package remapping configuration, and full Solidity compiler semantics are outside this coverage. See the [Solidity coverage record](docs/design/dynamic-dispatch-coverage-playbook.md#solidity-static-coverage) for validation limits.

**Terraform/OpenTofu (`.tf`, `.tofu`):** Search declarations and follow static references, local module inputs and outputs, provider aliases, and unambiguous Cloud Posse component remote-state outputs. Dynamic module sources, remote module loading, and Terraform evaluation are outside this coverage. See the [Terraform coverage record](docs/design/dynamic-dispatch-coverage-playbook.md#terraform-static-coverage) for validation limits.

**Nix (`.nix`):** Index named bindings, functions, attribute sets, and inherited attributes; follow static calls and project-relative `import`, `builtins.import`, `callPackage`, and `callPackages` paths, including directory `default.nix` files. Dynamic paths, dynamic attributes, partial applications, and runtime module composition remain outside the supported flow coverage. See the [Nix coverage record](docs/design/dynamic-dispatch-coverage-playbook.md#nix-static-coverage) for evidence and validation limits.

**Component, template, and config formats:** Razor/Blazor, Svelte, Vue, Astro, Liquid, YAML, Twig, XML, and Java `.properties`.

Language support is selected automatically from file extensions. RustCodeGraph skips common dependency, build, and cache directories, honors `.gitignore`, and ignores files larger than 1 MB by default.

## Supported Frameworks and Bridges

**Backend and web routes:** Django, Flask, FastAPI, Express, NestJS, Laravel, Drupal, Rails, Spring, Play Framework, Gin, chi, gorilla/mux, Axum, actix, Rocket, ASP.NET, and Vapor.

**Frontend routes and components:** React, React Router, Next.js, Svelte/SvelteKit, Vue/Vue Router/Nuxt, and Astro.

**Native, mobile, and cross-language flows:** SwiftUI, UIKit, Swift/Objective-C bridging, React Native legacy bridge, React Native TurboModules, React Native native events, Expo Modules, and Fabric/Paper native views.

**Workspace conventions:** Cargo workspaces, TypeScript path aliases, SvelteKit `$lib`, and Nuxt/Vue auto imports.

## Troubleshooting

**`RustCodeGraph not initialized`**
Run `rustcodegraph init -i` in the project directory.

**Indexing is slow**
Make sure large generated or dependency directories are ignored by `.gitignore`.

**MCP server does not connect**
Agents start the server themselves. Check `rustcodegraph status`, then re-run `rustcodegraph install` if the agent configuration looks stale.

**Migrating from the TypeScript CodeGraph version**
Install `rustcodegraph` and make sure your MCP configuration points at the Rust binary. Existing TypeScript-era CodeGraph configuration is not reused automatically.

## Building from Source

Clone with submodules and prepare the two patched grammars before using Cargo:

```bash
git clone https://github.com/hunzhiwange/rustcodegraph.git
cd rustcodegraph
git submodule update --init
npm run prepare:grammars
cargo build --release
```

After pulling updates, run `git submodule update --init` and prepare
again. Preparation uses Git, Node.js 18+, npm, and a pinned tree-sitter generator;
installed binaries do not require them. See [grammar dependencies](grammars/README.md)
for source-package builds, licenses, and adding languages without copying parser
sources into this repository.

## More

- [User documentation](docs/user/README.md)
- [Changelog](CHANGELOG.md)

## License

MIT
