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

**Source languages:** TypeScript/TSX, JavaScript/JSX, Python, Go, Rust, Java, C, C++, C#, PHP, Ruby, Swift, Kotlin, Dart, Pascal/Delphi, Scala, Lua, Luau, Objective-C, R, Nix, Terraform/OpenTofu, Solidity, Erlang, VB.NET, COBOL, CFML, ArkTS, CUDA, and Metal (via C++).

**Metal (`.metal`):** Search shader declarations and follow static calls using the existing C++ parser. Supported trailing attributes are masked while preserving source positions. Full Metal language semantics, GPU runtime behavior, and external SDK resolution are outside this coverage. Local header references stay connected through incremental sync. See the [Metal coverage record](docs/design/dynamic-dispatch-coverage-playbook.md#metal-static-coverage) for validation status and limits.

**CUDA (`.cu`, `.cuh`):** Search declarations and follow static host-to-kernel and device-function calls using the existing C++ parser. Local header dependencies stay connected through incremental sync. CUDA runtime scheduling, GPU execution, and external SDK resolution are outside this coverage. See the [CUDA coverage record](docs/design/dynamic-dispatch-coverage-playbook.md#cuda-static-coverage) for limits and validation status.

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

**HTTP cross-tier (TypeScript/JavaScript):** Follow literal `fetch` and imported axios method calls into exact, unambiguous Express routes within the same package, including statically resolved router mounts. Dynamic URLs, route-parameter matching, axios instances, and cross-package delivery are outside this coverage. See the [HTTP coverage record](docs/design/dynamic-dispatch-coverage-playbook.md#http-tier-static-coverage) for local validation and limits.

**Queue cross-tier (TypeScript/JavaScript, Bull/BullMQ):** Follow literal `queue.add` jobs through import-backed named queues into unique named Bull `process` or BullMQ `Worker` handlers and their static helpers within the same package. Nest decorators, anonymous handlers, dynamic names, re-export chains, and runtime broker delivery are outside this coverage. See the [queue coverage record](docs/design/dynamic-dispatch-coverage-playbook.md#queue-tier-static-coverage) for binding rules, local validation, and limits.

**Socket cross-tier (TypeScript/JavaScript, Socket.IO):** Follow literal `emit` events in both client/server directions into import-backed `on`/`once` listeners and their static helpers within the same package. Named listeners may be local or directly imported; inline callbacks link to their enclosing named callable. NestJS gateway decorators, dynamic events, transport lifecycle events, rooms/namespaces, and runtime delivery are outside this coverage. See the [Socket coverage record](docs/design/dynamic-dispatch-coverage-playbook.md#socket-tier-static-coverage) for direction evidence, local validation, and limits.

**Tauri IPC (Rust and TypeScript/JavaScript):** Follow import-backed literal `invoke` calls into uniquely registered Rust commands, and literal `emit` events into `listen`/`once` handlers and their static helpers within the same app. Rust event receivers require explicitly typed `AppHandle` or `App` parameters. Dynamic names, window-targeted events, tauri-specta clients, and runtime delivery are outside this coverage. See the [Tauri coverage record](docs/design/dynamic-dispatch-coverage-playbook.md#tauri-static-coverage) for registration rules, local validation, and limits.

**Redux Toolkit and RTK Query:** Follow import-backed `createAsyncThunk` dispatch chains and React query, lazy-query, and mutation hooks destructured from a same-module `createApi` registration into literal endpoint callbacks. Dynamic registration, general Redux reducer/store execution, and RTK runtime scheduling are outside this coverage. See the [Redux/RTK coverage record](docs/design/dynamic-dispatch-coverage-playbook.md#redux-rtk-static-coverage) for local validation and limits.

**Pinia and Vuex stores (TypeScript/TSX and JavaScript/JSX):** Follow import-backed Pinia Options store instances into registered actions, and Vuex literal `dispatch`/`commit` calls through statically registered root stores and namespaced modules into actions and mutations. Pinia setup stores, dynamic registration or keys, mapping helpers, and reactive state-to-render execution are outside this coverage. See the [Vue store coverage record](docs/design/dynamic-dispatch-coverage-playbook.md#vue-store-static-coverage) for local validation and limits.

**Celery (Python `.py`):** Follow `delay` and `apply_async` calls on local or explicitly imported registered tasks into task bodies and their static helper calls. Dynamic task names, canvas workflows, broker delivery, and worker scheduling are outside this coverage. See the [Celery coverage record](docs/design/dynamic-dispatch-coverage-playbook.md#celery-static-coverage) for registration rules, local validation, and limits.

**MediatR (C# `.cs`):** Follow `Send(new Request(...))` and `Publish(new Notification(...))` into statically registered handlers and their helper calls within the same solution or project scope. Request type spellings must match exactly; variable arguments, mapper results, open generic handlers, and runtime pipeline behavior are outside this coverage. See the [MediatR coverage record](docs/design/dynamic-dispatch-coverage-playbook.md#mediatr-static-coverage) for receiver heuristics, ambiguity rules, local validation, and limits.

**Spring events (Java `.java`):** Follow inline `publishEvent(new Event(...))` calls into parameter-typed `@EventListener` and `@TransactionalEventListener` methods and their static helpers within the same application scope. Variable arguments, annotation-only event types, wildcard-only type resolution, Kotlin listeners, and runtime delivery or transaction ordering are outside this coverage. See the [Spring event coverage record](docs/design/dynamic-dispatch-coverage-playbook.md#spring-event-static-coverage) for receiver heuristics, type resolution, local validation, and limits.

**Laravel events (PHP `.php`):** Follow `event(new Event(...))` and `\event(new Event(...))` into typed `handle` methods or literal `$listen` mappings within the same application scope, then through static helper calls. Variable events, facade dispatch, runtime registration, queue delivery, and inheritance matching are outside this coverage. See the [Laravel event coverage record](docs/design/dynamic-dispatch-coverage-playbook.md#laravel-event-static-coverage) for namespace resolution, heuristics, local validation, and limits.

**Sidekiq (Ruby `.rb`):** Follow literal `perform_async`, `perform_in`, and `perform_at` calls into directly registered worker `perform` bodies and their static helpers within the same application tree. Dynamic receivers, inherited registrations, ActiveJob, and runtime queue delivery are outside this coverage. See the [Sidekiq coverage record](docs/design/dynamic-dispatch-coverage-playbook.md#sidekiq-static-coverage) for namespace rules, local validation, and limits.

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
