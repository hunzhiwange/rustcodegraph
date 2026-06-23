<div align="center">

# RustCodeGraph

English · [简体中文](README.zh-CN.md)

## 🎉 1.0 Released!

Already installed? Run `rustcodegraph upgrade` to update in place.

Follow [@getcodegraph](https://x.com/getcodegraph) on X for updates.

### Supercharge Claude Code, Cursor, Codex, OpenCode, Hermes Agent, Gemini, Antigravity, and Kiro with Semantic Code Intelligence

**~16% cheaper · ~58% fewer tool calls · 100% local**

### [Documentation →](docs/user/README.md)

[![npm version](https://img.shields.io/npm/v/rustcodegraph.svg)](https://www.npmjs.com/package/rustcodegraph)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Self-contained](https://img.shields.io/badge/Rust-native%20%C2%B7%20no%20Node%20runtime-brightgreen.svg)](https://github.com/hunzhiwange/rustcodegraph/releases)

[![Windows](https://img.shields.io/badge/Windows-supported-blue.svg)](#supported-platforms)
[![macOS](https://img.shields.io/badge/macOS-supported-blue.svg)](#supported-platforms)
[![Linux](https://img.shields.io/badge/Linux-supported-blue.svg)](#supported-platforms)

[![Claude Code](https://img.shields.io/badge/Claude_Code-supported-blueviolet.svg)](#supported-agents)
[![Cursor](https://img.shields.io/badge/Cursor-supported-blueviolet.svg)](#supported-agents)
[![Codex](https://img.shields.io/badge/Codex-supported-blueviolet.svg)](#supported-agents)
[![opencode](https://img.shields.io/badge/opencode-supported-blueviolet.svg)](#supported-agents)
[![Hermes Agent](https://img.shields.io/badge/Hermes_Agent-supported-blueviolet.svg)](#supported-agents)
[![Gemini](https://img.shields.io/badge/Gemini-supported-blueviolet.svg)](#supported-agents)
[![Antigravity](https://img.shields.io/badge/Antigravity-supported-blueviolet.svg)](#supported-agents)
[![Kiro](https://img.shields.io/badge/Kiro-supported-blueviolet.svg)](#supported-agents)

<br>

**The RustCodeGraph platform is coming** — for every PR, know exactly what to test, what could break, which flows are affected, and whether business logic is compromised.

</div>

## Get Started

### 1. Install the CLI

**No Node.js required** — one command grabs the right build for your OS:

```bash
# macOS / Linux
curl -fsSL https://raw.githubusercontent.com/hunzhiwange/rustcodegraph/main/install.sh | sh

# Homebrew (macOS / Linux)
brew install hunzhiwange/tap/rustcodegraph

# Windows (PowerShell)
irm https://raw.githubusercontent.com/hunzhiwange/rustcodegraph/main/install.ps1 | iex
```

<details>
<summary><b>Already have Node? Use npm instead</b></summary>

```bash
npm i -g rustcodegraph
```

<sub>The npm package downloads the matching Rust binary from GitHub Releases — nothing to compile, no TypeScript runtime to build. The installer puts `rustcodegraph` on your PATH but **doesn't change your current shell** — open a new terminal before the next step so the command resolves.</sub>

<sub>**Upgrade any time** with `rustcodegraph upgrade` — it detects how you installed (bundle, npm, or npx) and updates in place. Add `--check` to see if an update is available, or `rustcodegraph upgrade <version>` to pin one.</sub>

</details>

### 2. Wire up your agent(s)

In a **new terminal**, run the installer to connect RustCodeGraph to the agents you use:

```bash
rustcodegraph install
```

<sub>Detects and auto-configures Claude Code, Cursor, Codex CLI, opencode, Hermes Agent, Gemini CLI, Antigravity IDE, and Kiro — wiring the RustCodeGraph MCP server into each. **This is the step that connects RustCodeGraph to your agent;** installing the CLI in step 1 does not do it on its own.</sub>

### 3. Initialize each project

```bash
cd your-project
rustcodegraph init -i
```

<sub>`rustcodegraph init -i` creates the local `.rustcodegraph/` directory and builds the full graph in the same step. Use plain `rustcodegraph init` only when you want to create the database without indexing yet.</sub>

<div align="center">

![1_C_VYnhpys0UHrOuOgpgoyw](https://github.com/user-attachments/assets/f168182f-4d9a-44e0-94d7-08d018cc8a3a)

</div>

### 4. No more syncing!

Auto-sync is enabled by default. RustCodeGraph watches the project and updates the graph on every file change — while your agent edits code, or you add, modify, or delete files. **The index is never stale, and there is nothing to re-run.**

### Uninstall

Changed your mind? One command removes RustCodeGraph from every agent it configured:

```bash
rustcodegraph uninstall
```

<sub>Reverses the installer — strips RustCodeGraph's MCP server config, instructions, and permissions from each configured agent. Your project indexes (`.rustcodegraph/`) are left untouched; remove those per-project with `rustcodegraph uninit`. If you installed through npm and also want to remove the package, run this before uninstalling `rustcodegraph`; npm package removal does not edit agent configs. Use `--target` to remove from specific agents, or `--yes` to run non-interactively.</sub>

---

## Why RustCodeGraph?

When Claude Code explores a codebase, it spawns **Explore agents** that scan files with grep, glob, and Read — consuming tokens on every tool call.

**RustCodeGraph gives those agents a pre-indexed knowledge graph** — symbol relationships, call graphs, and code structure. Agents query the graph instantly instead of scanning files.

### Benchmark Results

Tested across **7 real-world open-source codebases** spanning 7 languages, comparing an agent (Claude Code, headless) answering one architecture question **with** and **without** RustCodeGraph. Each cell is the savings at the **median of 4 runs per arm**. _Re-validated on Opus 4.8 (2026-06-02), on the current build (`rustcodegraph_explore` as the primary tool)._

> **Average: 16% cheaper · 47% fewer tokens · 22% faster · 58% fewer tool calls**

| Codebase | Language | Cost | Tokens | Time | Tool calls |
|----------|----------|------|--------|------|------------|
| **VS Code** | TypeScript · ~10k files | 18% cheaper | 64% fewer | 11% faster | 81% fewer |
| **Excalidraw** | TypeScript · ~640 | even | 25% fewer | 27% faster | 40% fewer |
| **Django** | Python · ~3k | 8% cheaper | 60% fewer | 13% faster | 77% fewer |
| **Tokio** | Rust · ~790 | even | 38% fewer | 18% faster | 57% fewer |
| **OkHttp** | Java · ~645 | 25% cheaper | 54% fewer | 31% faster | 50% fewer |
| **Gin** | Go · ~110 | 19% cheaper | 23% fewer | 24% faster | 44% fewer |
| **Alamofire** | Swift · ~110 | 40% cheaper | 64% fewer | 33% faster | 58% fewer |

RustCodeGraph cuts **tokens, tool calls, and wall-clock time on every repo** — across small, medium, and large codebases — and answers them with **near-zero file reads**, while the no-RustCodeGraph agent spends its budget on grep/find/Read discovery. `rustcodegraph_explore` shows the answer in full — the mechanism plus the exact methods you asked about, even when they're buried in a multi-thousand-line file — while collapsing redundant interchangeable implementations to signatures, so the response is sized to the *answer* rather than the file count. **Cost stays flat-to-cheaper everywhere** — largest on the small repos (Alamofire, OkHttp), roughly break-even on the most response-heavy ones (Excalidraw, Tokio), where RustCodeGraph trades the no-RustCodeGraph agent's many small grep/read round-trips for a few large, cache-heavy tool responses.

<details>
<summary><strong>Per-repo breakdown — WITH vs WITHOUT (median of 4)</strong></summary>

**VS Code** · ~10k files
| Metric | WITH cg | WITHOUT cg | Δ |
|---|---|---|---|
| Time | 1m 59s | 2m 13s | 11% faster |
| File Reads | 0 | 9 | −9 |
| Grep/Bash | 0 | 11 | −11 |
| Tool calls | 4 | 21 | 81% fewer |
| Total tokens | 640k | 1.79M | 64% fewer |
| Cost | $0.68 | $0.83 | 18% cheaper |

**Excalidraw** · ~640 files
| Metric | WITH cg | WITHOUT cg | Δ |
|---|---|---|---|
| Time | 1m 32s | 2m 6s | 27% faster |
| File Reads | 0 | 7 | −7 |
| Grep/Bash | 1 | 8 | −7 |
| Tool calls | 9 | 15 | 40% fewer |
| Total tokens | 1.27M | 1.69M | 25% fewer |
| Cost | $0.78 | $0.78 | even |

**Django** · ~3k files
| Metric | WITH cg | WITHOUT cg | Δ |
|---|---|---|---|
| Time | 1m 43s | 1m 58s | 13% faster |
| File Reads | 0 | 9 | −9 |
| Grep/Bash | 0 | 5 | −5 |
| Tool calls | 3 | 13 | 77% fewer |
| Total tokens | 559k | 1.41M | 60% fewer |
| Cost | $0.57 | $0.62 | 8% cheaper |

**Tokio** · ~790 files
| Metric | WITH cg | WITHOUT cg | Δ |
|---|---|---|---|
| Time | 1m 55s | 2m 20s | 18% faster |
| File Reads | 0 | 8 | −8 |
| Grep/Bash | 0 | 6 | −6 |
| Tool calls | 6 | 14 | 57% fewer |
| Total tokens | 1.08M | 1.73M | 38% fewer |
| Cost | $0.82 | $0.82 | even |

**OkHttp** · ~645 files
| Metric | WITH cg | WITHOUT cg | Δ |
|---|---|---|---|
| Time | 1m 1s | 1m 29s | 31% faster |
| File Reads | 0 | 4 | −4 |
| Grep/Bash | 2 | 6 | −4 |
| Tool calls | 5 | 10 | 50% fewer |
| Total tokens | 502k | 1.10M | 54% fewer |
| Cost | $0.41 | $0.55 | 25% cheaper |

**Gin** · ~110 files
| Metric | WITH cg | WITHOUT cg | Δ |
|---|---|---|---|
| Time | 1m 14s | 1m 37s | 24% faster |
| File Reads | 1 | 6 | −5 |
| Grep/Bash | 1 | 2 | −1 |
| Tool calls | 5 | 9 | 44% fewer |
| Total tokens | 651k | 847k | 23% fewer |
| Cost | $0.46 | $0.57 | 19% cheaper |

**Alamofire** · ~110 files
| Metric | WITH cg | WITHOUT cg | Δ |
|---|---|---|---|
| Time | 1m 35s | 2m 21s | 33% faster |
| File Reads | 0 | 9 | −9 |
| Grep/Bash | 0 | 4 | −4 |
| Tool calls | 5 | 12 | 58% fewer |
| Total tokens | 766k | 2.10M | 64% fewer |
| Cost | $0.57 | $0.95 | 40% cheaper |

</details>

<details>
<summary><strong>Full benchmark details</strong></summary>

**Methodology.** Each arm is `claude -p` (Claude Opus 4.8) run headlessly against the repo with `--strict-mcp-config`: **WITH** = RustCodeGraph's MCP server enabled, **WITHOUT** = an empty MCP config. Built-in Read/Grep/Bash stay available to both. Same question per repo, **4 runs per arm, median reported**. Cost = the run's `total_cost_usd`; Tokens = total tokens processed (input incl. cached + output); Time = wall-clock; Tool calls = every tool invocation, including those inside any sub-agents the model spawns. Repos cloned at `--depth 1` and indexed by the same RustCodeGraph build that served them. Re-validated 2026-06-02 on the current build. These numbers are lower than the prior Opus 4.7 validation — not a RustCodeGraph regression but a stronger native baseline: Opus 4.8 greps/reads efficiently on the main thread instead of fanning out into large Explore-subagent sweeps, so the no-RustCodeGraph arm is leaner than it used to be. Per-repo numbers move run-to-run with how hard the without-arm thrashes (the median-of-4 smooths it, but tails remain — e.g. Django's without-arm hit $2.71/14m one batch).

**Queries:**
| Codebase | Query |
|----------|-------|
| VS Code | "How does the extension host communicate with the main process?" |
| Excalidraw | "How does Excalidraw render and update canvas elements?" |
| Django | "How does Django's ORM build and execute a query from a QuerySet?" |
| Tokio | "How does tokio schedule and run async tasks on its runtime?" |
| OkHttp | "How does OkHttp process a request through its interceptor chain?" |
| Gin | "How does gin route requests through its middleware chain?" |
| Alamofire | "How does Alamofire build, send, and validate a request?" |

**Why RustCodeGraph wins:** with the index available, the agent answers directly — usually one `rustcodegraph_explore` returns the relevant source — and stops, usually with zero file reads. Without it, the agent spends most of its budget on discovery (find/ls/grep) before reading the right code. RustCodeGraph only helps when queried *directly*, so its instructions steer agents to answer directly rather than delegate exploration to file-reading sub-agents — otherwise a sub-agent reads files regardless and RustCodeGraph becomes overhead.

</details>

---

## Key Features

| | |
|---|---|
| **Smart Context Building** | One tool call returns entry points, related symbols, and code snippets — no expensive exploration agents |
| **Full-Text Search** | Find code by name instantly across your entire codebase, powered by FTS5 |
| **Impact Analysis** | Trace callers, callees, and the full impact radius of any symbol before making changes |
| **Always Fresh** | File watcher uses native OS events (FSEvents/inotify/ReadDirectoryChangesW) with debounced auto-sync — the graph stays current as you code, zero config |
| **20+ Languages** | TypeScript, JavaScript, Python, Go, Rust, Java, C#, PHP, Ruby, C, C++, Objective-C, Swift, Kotlin, Scala, Dart, Lua, Luau, R, Svelte, Vue, Astro, Liquid, Pascal/Delphi |
| **Framework-aware Routes** | Recognizes web-framework routing files and links URL patterns to their handlers across 17 frameworks |
| **Mixed iOS / React Native / Expo** | Closes cross-language flows that static parsing misses: Swift ↔ ObjC bridging, React Native legacy bridge + TurboModules + Fabric view components, native → JS event emitters, Expo Modules |
| **100% Local** | No data leaves your machine. No API keys. No external services. SQLite database only |

<details>
<summary><strong>How auto-syncing works — and why you don't need to run <code>rustcodegraph sync</code> manually</strong></summary>

When your agent (Claude Code, Cursor, Codex, opencode) launches `rustcodegraph serve --mcp`, three layers keep the index in step with your code — and make sure the agent never gets a silent wrong answer in the brief window between an edit and the next sync:

1. **File watcher with debounced auto-sync.** A native FSEvents / inotify / ReadDirectoryChangesW watcher captures every source-file create / modify / delete and triggers a re-index after a debounce window (default `2000ms`, tunable via `RUSTCODEGRAPH_WATCH_DEBOUNCE_MS`, clamped to `[100ms, 60s]`). Bursts of edits collapse into a single sync.

2. **Per-file staleness banner.** During the brief debounce window, MCP tool responses that would reference a still-pending file prepend a `⚠️` banner naming it and telling the agent to `Read` it directly. Pending files NOT referenced by the response surface as a small footer instead. Either way, the agent gets an explicit signal — validated with Claude Code, where the agent literally says "Reading the file directly for the live content" before opening it.

3. **Connect-time catch-up.** When the MCP server (re)connects, RustCodeGraph runs a fast `(size, mtime)` + content-hash reconciliation against the working tree before answering the first query — so edits made while no MCP server was running (a `git pull` from the terminal, edits from another editor, a previous agent session that exited) get absorbed on the next session's first tool call.

```
agent writes src/Widget.ts
  → watcher fires (<100ms)
  → debounce (default 2s)
  → sync; Widget.ts is in the index
  → next agent query sees it
```

**Verify any time** with `rustcodegraph_status` (via MCP) or `rustcodegraph status` (CLI). If anything is pending, you'll see a `### Pending sync:` section naming the files and their edit age.

The handful of cases where manual `rustcodegraph sync` makes sense: the watcher is disabled (sandboxed environments, or `RUSTCODEGRAPH_NO_DAEMON=1`), or you're scripting against the index outside an agent session and want a pre-flight sync at the start of your script.

→ Full deep-dive in [Guides → Indexing a Project](docs/user/guides/indexing.md#stay-fresh-automatically).

</details>

---

## Framework-aware Routes

RustCodeGraph detects web-framework routing files and emits `route` nodes linked by `references` edges to their handler classes or functions. Querying callers of a view/controller now surfaces the URL pattern that binds it.

| Framework | Shapes recognized |
|---|---|
| **Django** | `path()`, `re_path()`, `url()`, `include()` in `urls.py` (CBV `.as_view()`, dotted paths) |
| **Flask** | `@app.route('/path', methods=[...])`, blueprint routes |
| **FastAPI** | `@app.get(...)`, `@router.post(...)`, all standard methods |
| **Express** | `app.get(...)`, `router.post(...)` with middleware chains |
| **NestJS** | `@Controller` + `@Get/@Post/...`, GraphQL `@Resolver` + `@Query/@Mutation`, `@MessagePattern`/`@EventPattern`, `@SubscribeMessage` |
| **Laravel** | `Route::get()`, `Route::resource()`, `Controller@action`, tuple syntax |
| **Drupal** | `*.routing.yml` routes (`_controller`, `_form`, entity handlers); `hook_*` implementations in `.module`/`.theme`/`.install`/`.inc` |
| **Rails** | `get '/x', to: 'users#index'`, hash-rocket `=>` syntax |
| **Spring** | `@GetMapping`, `@PostMapping`, `@RequestMapping` on methods |
| **Play** | `GET`/`POST`/… verb routes in `conf/routes` → `Controller.method` actions (Scala + Java) |
| **Gin / chi / gorilla / mux** | `r.GET(...)`, `router.HandleFunc(...)` |
| **Axum / actix / Rocket** | `.route("/x", get(handler))` |
| **ASP.NET** | `[HttpGet("/x")]` attributes on action methods |
| **Vapor** | `app.get("x", use: handler)` |
| **React Router** / **SvelteKit** | Route component nodes |
| **Vue Router** / **Nuxt** | `pages/` file-based routes, `server/api/` endpoints, route middleware |
| **Astro** | `src/pages/` file-based routes (`.astro` pages + `.ts` endpoints, `[param]`/`[...rest]` syntax) |

---

## Mixed iOS / React Native / Expo bridging

Real iOS and React Native codebases live across multiple languages — a Swift caller invokes an Objective-C selector that's been auto-bridged, a JS file calls into a native module via the React Native bridge, a JSX component delegates to a native view manager. Static tree-sitter extraction stops at each language boundary. RustCodeGraph bridges them so `rustcodegraph_explore`, `rustcodegraph_callers`, `rustcodegraph_callees`, and `rustcodegraph_impact` connect end-to-end across the gap.

| Boundary | JS / Swift side | Native side | How |
|---|---|---|---|
| **Swift → ObjC** | Swift `obj.foo(bar:)` | ObjC selector `-fooWithBar:` | `@objc` auto-bridging rules (including init/property/protocol forms) + Cocoa preposition prefixes (`With`/`For`/`By`/`In`/`On`/`At`/…) |
| **ObjC → Swift** | ObjC `[obj fooWithBar:]` | Swift `@objc func foo(bar:)` | Reverse-bridge name candidates; verifies `@objc` exposure from source |
| **React Native legacy bridge** | JS `NativeModules.X.fn(...)` | ObjC `RCT_EXPORT_METHOD` / `RCT_REMAP_METHOD` · Java/Kotlin `@ReactMethod` | Parses macro/annotation declarations to build a JS-name → native-method map |
| **React Native TurboModules** | JS `import M from './NativeM'; M.fn(...)` | Native impl matching the Codegen spec | Treats the `Native<X>.ts` spec interface as ground truth |
| **RN native → JS events** | JS `new NativeEventEmitter(...).addListener('e', cb)` | ObjC `[self sendEventWithName:@"e" body:...]` · Swift `sendEvent(withName: "e", ...)` · Java/Kotlin `.emit("e", ...)` | Synthesized cross-language event channel keyed by literal event name |
| **Expo Modules** | JS `requireNativeModule('X').fn(...)` | Swift / Kotlin `Module { Name("X"); AsyncFunction("fn") { ... } }` | Parses the Expo DSL literals; synthetic method nodes resolve via existing name-match |
| **Fabric view components** | JSX `<MyView prop={v}/>` | TS Codegen spec + native impl class | Spec → `component` node; convention-based name+suffix lookup (`View`/`ComponentView`/`Manager`/`ViewManager`) bridges to native |
| **Legacy Paper view managers** | JSX `<MyView prop={v}/>` | ObjC `RCT_EXPORT_VIEW_PROPERTY` · Java/Kotlin `@ReactProp` | Same as Fabric — Paper-era declarations also produce `component` + `property` nodes |

**Validated on real codebases** (small + medium + large for each bridge):

| Bridge | Small | Medium | Large |
|---|---|---|---|
| Swift ↔ ObjC | [Charts](https://github.com/danielgindi/Charts) | [realm-swift](https://github.com/realm/realm-swift) | [Wikipedia-iOS](https://github.com/wikimedia/wikipedia-ios) |
| RN legacy bridge | [AsyncStorage](https://github.com/react-native-async-storage/async-storage) | [react-native-svg](https://github.com/software-mansion/react-native-svg) | [react-native-firebase](https://github.com/invertase/react-native-firebase) |
| RN native → JS events | [RNGeolocation](https://github.com/Agontuk/react-native-geolocation-service) | — | react-native-firebase |
| Expo Modules | expo-haptics | expo-camera | expo SDK sweep (7 packages) |
| Fabric / Paper views | [react-native-segmented-control](https://github.com/react-native-segmented-control/segmented-control) | [react-native-screens](https://github.com/software-mansion/react-native-screens) | [react-native-skia](https://github.com/Shopify/react-native-skia) |

Each bridge emits edges tagged `provenance:'heuristic'` with `metadata.synthesizedBy:` set to a stable channel name (e.g. `swift-objc-bridge`, `rn-event-channel`, `fabric-native-impl`, `expo-module-extract`), so the agent can tell at a glance how a hop got into the graph.

---

## Quick Start

### 1. Run the Installer

```bash
rustcodegraph install
```

The installer will:
- Ask which agent(s) to configure — auto-detects installed ones from: **Claude Code**, **Cursor**, **Codex CLI**, **opencode**, **Hermes Agent**, **Gemini CLI**, **Antigravity IDE**, **Kiro**
- Prompt to install `rustcodegraph` on your PATH (so agents can launch the MCP server)
- Ask whether configs apply to all your projects or just this one
- Write each chosen agent's MCP server config, plus the small RustCodeGraph guidance block used by that agent target when needed. Removed cleanly by `rustcodegraph uninstall`.
- Set up auto-allow permissions when Claude Code is one of the targets
- Initialize your current project (local installs only)

**Non-interactive (scripting / CI):**

```bash
rustcodegraph install --yes                              # auto-detect agents, install global
rustcodegraph install --target=cursor,claude --yes       # explicit target list
rustcodegraph install --target=auto --location=local     # detected agents, project-local
rustcodegraph install --print-config codex               # print snippet, no file writes
```

| Flag | Values | Default |
|---|---|---|
| `--target` | `auto`, `all`, `none`, or csv (`claude,cursor,...`) | prompt |
| `--location` | `global`, `local` | prompt |
| `--yes` | (boolean) | prompt every step |
| `--no-permissions` | (boolean) skip Claude auto-allow list | permissions on |
| `--print-config <id>` | dump snippet for one agent and exit | — |

### 2. Restart Your Agent

Restart your agent (Claude Code / Cursor / Codex CLI / opencode / Hermes Agent / Gemini CLI / Antigravity IDE / Kiro) for the MCP server to load.

### 3. Initialize Projects

```bash
cd your-project
rustcodegraph init -i
```

Builds the per-project knowledge graph index, which then auto-syncs on every file change. A single global `rustcodegraph install` works in every project you open — no need to re-run the installer per project.

That's it — your agent will use RustCodeGraph tools automatically when a `.rustcodegraph/` directory exists.

<details>
<summary><strong>Manual Setup (Alternative)</strong></summary>

**Install globally:**
```bash
# macOS / Linux
curl -fsSL https://raw.githubusercontent.com/hunzhiwange/rustcodegraph/main/install.sh | sh

# Homebrew (macOS / Linux)
brew install hunzhiwange/tap/rustcodegraph

# Or use npm.
npm install -g rustcodegraph
```

**Add to `~/.claude.json`:**
```json
{
  "mcpServers": {
    "rustcodegraph": {
      "type": "stdio",
      "command": "rustcodegraph",
      "args": ["serve", "--mcp"]
    }
  }
}
```

**Add to `~/.claude/settings.json` (optional, for auto-allow):**
```json
{
  "permissions": {
    "allow": [
      "mcp__rustcodegraph__rustcodegraph_search",
      "mcp__rustcodegraph__rustcodegraph_explore",
      "mcp__rustcodegraph__rustcodegraph_callers",
      "mcp__rustcodegraph__rustcodegraph_callees",
      "mcp__rustcodegraph__rustcodegraph_impact",
      "mcp__rustcodegraph__rustcodegraph_node",
      "mcp__rustcodegraph__rustcodegraph_status",
      "mcp__rustcodegraph__rustcodegraph_files"
    ]
  }
}
```

</details>

<details>
<summary><strong>Agent Tool Guidance</strong></summary>

RustCodeGraph's MCP server delivers its usage guidance to your agent **automatically**, in the MCP `initialize` response. In short, it tells the agent to:

- **Answer structural questions directly with RustCodeGraph** — it *is* the pre-built index, so a grep/read loop just repeats work it already did. Treat the returned source as already read.
- **Pick the tool by intent:** `rustcodegraph_explore` for almost anything — "how does X work", a flow/"how does X reach Y", or surveying an area (one call returns the relevant symbols' source grouped by file); `rustcodegraph_search` to just locate a symbol; `rustcodegraph_callers` for every call site (including callback registrations); `rustcodegraph_node` for one symbol's full source + callers, or to read a file like the Read tool.
- **Trust the results — don't re-verify with grep**, and check the staleness banner after edits.
- In a workspace with no index, RustCodeGraph announces itself inactive and serves no tools — indexing stays your decision.

The exact text is `src/mcp/server_instructions.rs` — the single source of truth for the main agent. Because subagents and non-MCP harnesses never see the MCP guidance, the installer also writes a short marker-fenced section into the agent's instructions file pointing at the `rustcodegraph explore` / `rustcodegraph node` CLI equivalents.

</details>

---

## How It Works

```
┌───────────────────────────────────────────────────────────────────┐
│                            Claude Code                            │
│                                                                   │
│   "How does a request reach the database?"                        │
│       calls RustCodeGraph tools directly — no Explore sub-agent       │
│                                 │                                 │
└─────────────────────────────────┬─────────────────────────────────┘
                                  │
                                  ▼
┌───────────────────────────────────────────────────────────────────┐
│                        RustCodeGraph MCP Server                       │
│                                                                   │
│       explore · search · callers · callees · impact · node        │
│                                 │                                 │
│                                 ▼                                 │
│                       SQLite knowledge graph                      │
│          symbols · edges · files · FTS5 full-text search          │
└───────────────────────────────────────────────────────────────────┘
```

1. **Extraction** — [tree-sitter](https://tree-sitter.github.io/) parses source code into ASTs. Language-specific queries extract nodes (functions, classes, methods) and edges (calls, imports, extends, implements).

2. **Storage** — Everything goes into a local SQLite database (`.rustcodegraph/rustcodegraph.db`) with FTS5 full-text search.

3. **Resolution** — After extraction, references are resolved: function calls → definitions, imports → source files, class inheritance, and framework-specific patterns.

4. **Auto-Sync** — The MCP server watches your project using native OS file events. Changes are debounced (2-second quiet window), filtered to source files only, and incrementally synced. The graph stays fresh as you code — no configuration needed.

---

## CLI Reference

```bash
rustcodegraph                     # Run interactive installer
rustcodegraph install                 # Run installer (explicit)
rustcodegraph uninstall               # Remove RustCodeGraph from your agents (inverse of install)
rustcodegraph init [path]             # Initialize only; add -i/--index to build the graph too
rustcodegraph uninit [path]           # Remove RustCodeGraph from a project (--force to skip prompt)
rustcodegraph index [path]            # Full index (--force to re-index, --quiet for less output)
rustcodegraph sync [path]             # Incremental update
rustcodegraph status [path]           # Show statistics
rustcodegraph unlock [path]           # Remove a stale lock file that's blocking indexing
rustcodegraph query <search>          # Search symbols (--kind, --limit, --json)
rustcodegraph explore <query>         # Relevant symbols' source + call paths in one shot (same output as the rustcodegraph_explore MCP tool)
rustcodegraph node <symbol|file>      # One symbol's source + callers, or read a file with line numbers (same output as rustcodegraph_node)
rustcodegraph files [path]            # Show file structure (--format, --filter, --max-depth, --json)
rustcodegraph callers <symbol>        # Find what calls a function/method (--limit, --json)
rustcodegraph callees <symbol>        # Find what a function/method calls (--limit, --json)
rustcodegraph impact <symbol>         # Analyze what code is affected by changing a symbol (--depth, --json)
rustcodegraph affected [files...]     # Find test files affected by changes (see below)
rustcodegraph daemon                  # Manage background daemons — pick one to stop (alias: daemons)
rustcodegraph telemetry [on|off]      # Show or change anonymous usage telemetry
rustcodegraph upgrade [version]       # Update to the latest release (--check, --force)
rustcodegraph version                 # Print the installed version (also -v, --version)
rustcodegraph help [command]          # Show help, optionally for one command
```

### `rustcodegraph affected`

Traces import dependencies transitively to find which test files are affected by changed source files.

```bash
rustcodegraph affected src/utils.ts src/api.ts         # Pass files as arguments
git diff --name-only | rustcodegraph affected --stdin   # Pipe from git diff
rustcodegraph affected src/auth.ts --filter "e2e/*"     # Custom test file pattern
```

| Option | Description | Default |
|--------|-------------|---------|
| `--stdin` | Read file list from stdin | `false` |
| `-d, --depth <n>` | Max dependency traversal depth | `5` |
| `-f, --filter <glob>` | Custom glob to identify test files | auto-detect |
| `-j, --json` | Output as JSON | `false` |
| `-q, --quiet` | Output file paths only | `false` |

**CI/hook example:**

```bash
#!/usr/bin/env bash
AFFECTED=$(git diff --name-only HEAD | rustcodegraph affected --stdin --quiet)
if [ -n "$AFFECTED" ]; then
  npx vitest run $AFFECTED
fi
```

---

## MCP Tools

When running as an MCP server, RustCodeGraph exposes a focused set of four tools — measured agent behavior showed a leaner list steers agents to the right tool and saves context every session:

| Tool | Purpose |
|------|---------|
| `rustcodegraph_explore` | **Primary.** Answer almost any question in one call — "how does X work", a flow ("how does X reach Y"), or surveying an area — returning the relevant symbols' verbatim source grouped by file, plus a relationship map and blast radius. Surfaces dynamic-dispatch hops (callbacks, React re-render, interface→impl) grep can't follow. |
| `rustcodegraph_node` | One symbol's full source + caller/callee trail (every overload for an ambiguous name) — or pass a file path to **read a whole file like the Read tool** (same line-numbered output, `offset`/`limit`), with its dependents attached. |
| `rustcodegraph_search` | Find symbols by name across the codebase |
| `rustcodegraph_callers` | Every call site of a function — including where it's registered as a callback — with one section per definition when several share a name |

Four more tools (`rustcodegraph_callees`, `rustcodegraph_impact`, `rustcodegraph_files`, `rustcodegraph_status`) stay fully functional but unlisted by default — measured across eval runs, agents never or rarely picked them, and their information already arrives inline on the four above (explore's blast-radius section, node's dependents note, a symbol's body as its callee list). Re-enable any of them with the `RUSTCODEGRAPH_MCP_TOOLS` environment variable (e.g. `RUSTCODEGRAPH_MCP_TOOLS=explore,node,search,callers,impact`), or use their CLI equivalents (`rustcodegraph callees` / `impact` / `files` / `status`).

In a workspace with no `.rustcodegraph/` index, the server announces itself inactive and lists **no** tools — agents work normally with their built-in tools, and indexing stays your decision.

---

## Library Usage

The production runtime is Rust. Rust callers can embed RustCodeGraph directly from
the crate. npm packages are CLI distribution packages only; they do not expose
`require()` or `import` entrypoints.

```rust
use rustcodegraph::{RustCodeGraph, IndexOptions};

let mut cg = RustCodeGraph::init_sync("/path/to/project")?;
cg.index_all(IndexOptions::default())?;

let results = cg.search_nodes("UserService", None);
let callers = cg.get_callers(&results[0].node.id);

cg.close();
```

Node.js integrations should shell out to `rustcodegraph` when they need the CLI;
there is no published JavaScript SDK surface.

---

## Configuration

There isn't any — RustCodeGraph is zero-config, with **no config file** to write or
keep in sync. Language support is automatic from the file extension; there's
nothing to wire up per language.

What it skips out of the box:

- **Dependency, build, and cache directories** — `node_modules`, `vendor`,
  `dist`, `build`, `target`, `.venv`, `Pods`, `.next`, and the like across every
  [supported stack](#supported-languages) — so the graph is your code, not
  third-party noise. This holds even with no `.gitignore`.
- **Anything in your `.gitignore`** — honored in git repos via git, and in
  non-git projects by reading `.gitignore` directly (root and nested).
- **Files larger than 1 MB** — generated bundles, minified JS, vendored blobs.

To keep something else out, add it to `.gitignore`. To pull a default-excluded
directory back **in** (say you really do want a vendored dependency indexed),
add a negation — `!vendor/`. The defaults apply uniformly, so committing a
dependency or build directory doesn't force it into the graph; the `.gitignore`
negation is the explicit opt-in.

## Telemetry

RustCodeGraph collects **anonymous usage statistics** — which tools and commands get
used, which languages get indexed — to guide where language and agent support
work goes. **Never** any code, paths, file or symbol names, queries, or IP
addresses; usage is aggregated locally into daily totals before anything is
sent, and the ingest endpoint is [public code in this repo](telemetry-worker/)
that enforces the documented field list. The installer asks up front; turn it
off any time:

```bash
rustcodegraph telemetry off    # or: RUSTCODEGRAPH_TELEMETRY=0, or DO_NOT_TRACK=1
```

[`TELEMETRY.md`](TELEMETRY.md) lists every field, with the off-switches and the
full data-handling story.

## Supported Platforms

Every release ships a self-contained native Rust build — nothing to compile and
no Node.js runtime required for the standalone installers — for all three
desktop OSes, on both Intel/AMD (x64) and ARM (arm64):

| Platform | Architectures | Install |
|----------|---------------|---------|
| Windows | x64, arm64 | PowerShell installer or npm |
| macOS | x64, arm64 | shell installer, Homebrew, or npm |
| Linux | x64, arm64 | shell installer, Homebrew, or npm |

See [Get Started](#get-started) for the one-line install commands.

## Supported Agents

The interactive installer auto-detects and configures each of these — wiring up
the MCP server and a short instruction marker for subagents/non-MCP harnesses:

- **Claude Code**
- **Cursor**
- **Codex CLI**
- **opencode**
- **Hermes Agent**
- **Gemini CLI**
- **Antigravity IDE**
- **Kiro**

## Supported Languages

| Language | Extension | Status |
|----------|-----------|--------|
| TypeScript | `.ts`, `.tsx` | Full support |
| JavaScript | `.js`, `.jsx`, `.mjs` | Full support |
| Python | `.py` | Full support |
| Go | `.go` | Full support |
| Rust | `.rs` | Full support |
| Java | `.java` | Full support |
| C# | `.cs` | Full support |
| PHP | `.php` | Full support |
| Ruby | `.rb` | Full support |
| C | `.c`, `.h` | Full support |
| C++ | `.cpp`, `.hpp`, `.cc` | Full support |
| Objective-C | `.m`, `.mm`, `.h` | Partial support (classes, protocols, methods, `@property`, `#import`, message sends; `.mm` ObjC++ may parse incompletely) |
| Swift | `.swift` | Full support |
| Kotlin | `.kt`, `.kts` | Full support |
| Scala | `.scala`, `.sc` | Full support (classes, traits, methods, type aliases, Scala 3 enums) |
| Dart | `.dart` | Full support |
| Svelte | `.svelte` | Full support (script extraction, Svelte 5 runes, SvelteKit routes) |
| Vue | `.vue` | Full support (script + script-setup extraction, Nuxt page/API/middleware routes) |
| Astro | `.astro` | Full support (frontmatter + script extraction, template component/call references, `src/pages/` routes) |
| Liquid | `.liquid` | Full support |
| Pascal / Delphi | `.pas`, `.dpr`, `.dpk`, `.lpr` | Full support (classes, records, interfaces, enums, DFM/FMX form files) |
| Lua | `.lua` | Full support (functions, methods with receivers, local variables, `require` imports, call edges) |
| R | `.R` `.r` | Full support (functions in every assignment form, S4/R5/R6 classes with methods, `library`/`require` imports, `source()` file references, call edges) |
| Luau | `.luau` | Full support (everything in Lua, plus `type`/`export type` aliases, typed signatures, and Roblox instance-path `require`) |

## Measured cross-file coverage

Impact and blast-radius queries are only as good as the dependency graph behind them, so coverage is measured rather than asserted. **Fair coverage** = the share of symbol-bearing source files that have at least one *resolved cross-file dependent* — something that imports, calls, references, or (through a framework convention) routes to them — on a real-world benchmark repo per language. The residual is always a genuine static-analysis frontier (runtime dynamic dispatch, reflection / DI containers, framework-convention entry points, vendored third-party code), never hidden by gaming the denominator.

| Language | Benchmark repo | Coverage |
|---|---|---|
| TypeScript / JavaScript | this repo | 95.8% |
| Python | psf/requests | 100% |
| Go | gin-gonic/gin | 96.6% |
| Rust | BurntSushi/ripgrep | 86.7% |
| Java | google/gson | 93.3% |
| C# | jbogard/MediatR | 85.2% |
| PHP | guzzle/guzzle | 100% |
| Ruby | sidekiq/sidekiq | 100% |
| C | redis/redis | 92.2% |
| C++ | google/leveldb | 94.8% |
| Objective-C | SDWebImage | 91.6% |
| Swift | Alamofire | 95.3% |
| Kotlin | square/okhttp | 96.2% |
| Scala | gatling/gatling | 91.2% |
| Dart | flutter/packages | 92.4% |
| Svelte / SvelteKit | sveltejs/realworld | 100% |
| Vue / Nuxt | nuxt/movies | 93.5% |
| Astro | xingwangzhe/stalux | 93.0% |
| Lua | nvim-telescope/telescope.nvim | 84.2% |
| Luau | dphfox/Fusion | 92.2% |
| Liquid | Shopify/dawn | 73.8% |
| Pascal / Delphi | PascalCoin | 77.4% |

Framework routing is validated the same way, on a canonical app per framework: Express 100%, FastAPI 98%, Flask 100%, NestJS 96.8%, Gin 96.5%, Axum 100%, Rocket 93.8%, Vapor 100%, Laravel 92%, Rails 89.6%, React Router 100% — and the convention/reflection-heavy ones at their honest static-analysis ceiling: ASP.NET 83.9%, Spring 83.3%, Drupal 78.9%, Play 76.3%, Django 74.1%. SvelteKit, Vue/Nuxt, and Astro use file-based routing, so their page/endpoint coverage is the Svelte/SvelteKit (100%), Vue/Nuxt (93.5%), and Astro (93.0% — every `src/pages/` file maps to a route node on the two validation repos) figures in the table above.

## Troubleshooting

**"RustCodeGraph not initialized"** — Run `rustcodegraph init -i` in your project directory first.

**Indexing is slow** — Check that `node_modules` and other large directories are excluded. Use `--quiet` to reduce output overhead.

**MCP hits `database is locked`** — current builds shouldn't: the Rust runtime uses SQLite in WAL mode, where concurrent reads normally do not block on a writer. If you still see it:

- **You're still running the old CodeGraph package or binary.** RustCodeGraph is a separate project and does not upgrade CodeGraph in place. Install RustCodeGraph separately — `curl -fsSL https://raw.githubusercontent.com/hunzhiwange/rustcodegraph/main/install.sh | sh` (macOS/Linux), `irm https://raw.githubusercontent.com/hunzhiwange/rustcodegraph/main/install.ps1 | iex` (Windows), or `npm i -g rustcodegraph` — then make sure your MCP config points at `rustcodegraph`.
- **`rustcodegraph status` shows `Journal:` other than `wal`** — WAL couldn't be enabled on this filesystem (common on network shares and WSL2 `/mnt`), so reads can block on writes. Move the project (with its `.rustcodegraph/` folder) onto a local disk.

**MCP server not connecting** — Your agent starts the server itself, so you don't launch it by hand. Make sure the project is initialized and indexed (`rustcodegraph status`) and that the path in your MCP config is correct. If it still won't connect, re-run `rustcodegraph install` to rewrite the config.

**Missing symbols** — The MCP server auto-syncs on save (wait a couple seconds). Run `rustcodegraph sync` manually if needed. Check that the file's language is supported and isn't inside a `.gitignore`d or default-excluded directory (e.g. `node_modules`, `dist`).

**Sharing one checkout between Windows and WSL** — Don't point both at the same `.rustcodegraph/`: the background-server lock and the SQLite index are tied to the OS that wrote them, and SQLite locking across the WSL2/Windows filesystem boundary is unreliable. Give each side its own index in the same tree by setting `RUSTCODEGRAPH_DIR` to a distinct name on one of them — e.g. `RUSTCODEGRAPH_DIR=.rustcodegraph-win` on Windows, leaving WSL on the default `.rustcodegraph`. RustCodeGraph skips any sibling `.rustcodegraph-*` directory when indexing and watching, so the two never trip over each other.

## Star History

<a href="https://www.star-history.com/?repos=hunzhiwange%2Frustcodegraph&type=date&legend=top-left">
 <picture>
   <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/chart?repos=hunzhiwange/rustcodegraph&type=date&theme=dark&legend=top-left" />
   <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/chart?repos=hunzhiwange/rustcodegraph&type=date&legend=top-left" />
   <img alt="Star History Chart" src="https://api.star-history.com/chart?repos=hunzhiwange/rustcodegraph&type=date&legend=top-left" />
 </picture>
</a>

## License

MIT

---

<div align="center">

**Made for AI coding agents — Claude Code, Cursor, Codex CLI, opencode, Hermes Agent, Gemini CLI, Antigravity IDE, and Kiro**

[Report Bug](https://github.com/hunzhiwange/rustcodegraph/issues) · [Request Feature](https://github.com/hunzhiwange/rustcodegraph/issues)

</div>
