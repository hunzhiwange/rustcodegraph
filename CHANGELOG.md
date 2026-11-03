# Changelog

## [Unreleased]

### New Features

- Kimi Code reinstall preserves custom launch settings and user switches, while uninstall preserves other servers, comments, and line endings.

- Connect Kimi Code to RustCodeGraph with local or global installation and support for `KIMI_CODE_HOME`.

- Grok Build uninstall now removes RustCodeGraph settings and environment overrides while preserving other servers, comments, and line endings.

- Grok Build users can install RustCodeGraph locally or globally while preserving existing MCP settings.

- DeepSeek Harness uninstall now removes RustCodeGraph from profile patches while preserving comments, custom settings on reinstall, and other servers.

- DeepSeek Harness users can install RustCodeGraph globally while preserving existing Cordis patches.

- Qwen Code users can install RustCodeGraph in project or global settings while preserving existing configuration.

- Windsurf users can now install RustCodeGraph globally while preserving existing MCP settings.

- Factory Droid now supports local and global MCP installation while preserving existing settings.

- IBM Bob now supports local and global MCP installation while preserving existing settings.

- Install RustCodeGraph globally for the GitHub Copilot plugin in JetBrains IDEs while preserving existing MCP configuration.

### Fixes

- Qwen Code uninstall removes its automatic permission rule while preserving user settings and other MCP servers.

- Windsurf reinstall and uninstall preserve custom launch settings, comments, and other MCP servers.

- Factory Droid reinstall and uninstall preserve custom launch settings, comments, and other MCP servers.

- JetBrains Copilot reinstall preserves custom launch settings, and uninstall keeps comments and other MCP servers.

- Copilot CLI reinstall and uninstall preserve user settings, comments, and other MCP servers.

### New Features

- Install RustCodeGraph for GitHub Copilot CLI using global MCP configuration, including custom COPILOT_HOME directories.

- Install RustCodeGraph for VS Code Copilot Chat with workspace or user MCP configuration while preserving comments and other servers.

- Indexing now limits parallel parsing by available memory while preserving file order in the resulting graph.

### Fixes

- Uninstalling VS Code Copilot MCP configuration now removes empty server containers while preserving comments, other servers, and user settings.

- Installer tests now isolate agent configuration overrides so existing user settings remain untouched.

- MCP configuration editing now preserves JSONC comments and existing user settings, and rejects damaged files without overwriting them.

- Large JavaScript and TypeScript files now retain complete class methods and render flows without spurious members from function bodies.
- Exploration now prioritizes complete connected flows and preserves their source ahead of long caller lists.

- File symbol maps now group repeated names and avoid repeating the file path while retaining every indexed line location.

- MCP exploration and symbol lookup now return the full indexed source and retain retrieval state throughout each connection.

- CLI indexing now uses the full language parser and reference resolution pipeline so code exploration retains symbols missed by the lightweight index.

- JavaScript and TypeScript calls now respect local shadowing while reusing binding analysis during indexing.

- Incremental sync now defers automatic WAL checkpoints during database writes to avoid repeatedly flushing the same data.

### Security

- MCP sessions now keep sensitive default project folders inactive instead of exposing their indexed contents.


## [1.2.11] - 2026-11-01

### New Features

- `rustcodegraph sync` now shows live scanning, parsing, resolution, and database progress so long incremental refreshes no longer appear stuck.

## [1.2.10] - 2026-09-02

### Fixes

- Fixed indexing and sync panics across TypeScript/React, Java, C#, Rust, Dart, Ruby, PHP/Drupal, and C/C++ sources when lookahead windows, summaries, or line calculations ended inside Chinese, emoji, or other multibyte UTF-8 characters.

## [1.2.9] - 2026-08-14

### Fixes

- Published a maintenance release with the latest packaging and release workflow updates.

## [1.2.8] - 2026-08-14

### Fixes

- Published a maintenance release with the latest packaging and release workflow updates.

## [1.2.6] - 2026-07-19

### Fixes

- `rustcodegraph upgrade` now installs the published native release when run from a source checkout instead of telling users to pull and rebuild the repository, and its version status consistently uses the RustCodeGraph name.

## [1.2.5] - 2026-07-17

### Fixes

- Fixed `rustcodegraph watch` on macOS and Windows creating a watcher thread for every directory after folder changes, so large projects keep a constant watcher footprint while copied folders still sync automatically.

## [1.2.4] - 2026-07-09

### Fixes

- Fixed the standalone macOS, Linux, and Windows installers so they can install current cargo-dist release archives without reporting a missing binary.

## [1.2.3] - 2026-07-09

### Fixes

- Published a maintenance release with the latest packaging and release workflow updates.

## [1.2.2] - 2026-07-09

### Fixes

- Published a maintenance release with the latest packaging and release workflow updates.

## [1.2.1] - 2026-07-08

### Fixes

- Published a maintenance release with the latest packaging and release workflow updates.

## [1.2.0] - 2026-07-01

### Fixes

- Fixed a tree-sitter AST conversion bug that could make `rustcodegraph watch` consume gigabytes of memory when syncing Rust files with chained calls or many nested syntax nodes.
- Fixed a watch starvation case where a large copy could leave thousands of pending file events stuck behind a stale high-memory reading, so no-op batches now clear normally and follow-up edits sync instead of waiting forever.
- `rustcodegraph sync` and terminal `rustcodegraph watch` now share the same incremental sync path, so manual and automatic refreshes stay consistent without rebuilding the whole project for every edit.
- Reduced memory and disk use when watching large projects: a single-file change no longer re-reads every source file in the project, so `rustcodegraph watch` stays lightweight on big repos.
- `rustcodegraph watch` no longer piles up back-to-back heavy syncs under bursts of rapid file changes, so a new batch waits for the previous one to finish and release its working memory before continuing. Tune with `RUSTCODEGRAPH_WATCH_MIN_SYNC_INTERVAL_MS`.
- `rustcodegraph watch` now keeps the graph up to date while a large folder is being copied in: previously the unbroken stream of file events could keep postponing the sync indefinitely, so changes were detected but never indexed. A maximum wait now guarantees the index catches up at a steady cadence even while files are still streaming in, tunable via `RUSTCODEGRAPH_WATCH_MAX_DEBOUNCE_MS`.
- `rustcodegraph watch` and MCP status messages now explain when files are waiting for the next batch sync, so longer watch windows no longer look like a stuck watcher or push agents back into file-reading fallbacks.
- `rustcodegraph watch` now reports the same added, modified, and removed file counts as `rustcodegraph sync`, and directory removals now trigger an automatic refresh even when the OS reports only the deleted directory path.

## [1.1.5] - 2026-06-27

### Fixes

- Published a maintenance release with the latest packaging and release workflow updates.

## [1.1.4] - 2026-06-27

### Fixes

- Published a maintenance release with the latest packaging and release workflow updates.

## [1.1.3] - 2026-06-27

### Fixes

- Published a maintenance release with the latest packaging and release workflow updates.

## [1.1.2] - 2026-06-27

### Fixes

- Published a maintenance release with the latest packaging and release workflow updates.

## [1.1.1] - 2026-06-27

### Fixes

- Published a maintenance release with the latest packaging and release workflow updates.

## [1.1.0] - 2026-06-27

### Fixes

- Published a maintenance release with the latest packaging and release workflow updates.
[1.2.0]: https://github.com/hunzhiwange/rustcodegraph/releases/tag/v1.2.0
[1.2.4]: https://github.com/hunzhiwange/rustcodegraph/releases/tag/v1.2.4
[1.2.5]: https://github.com/hunzhiwange/rustcodegraph/releases/tag/v1.2.5
[1.2.6]: https://github.com/hunzhiwange/rustcodegraph/releases/tag/v1.2.6
[1.2.10]: https://github.com/hunzhiwange/rustcodegraph/releases/tag/v1.2.10
[1.2.11]: https://github.com/hunzhiwange/rustcodegraph/releases/tag/v1.2.11
