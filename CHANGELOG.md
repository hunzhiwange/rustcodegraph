# Changelog

## [Unreleased]


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
