# Rust Public API Surface

Updated: 2026-06-21

This audit compares the TypeScript package entry point in `src/index.ts` with
the Rust crate entry point in `src/index.rs`.

## RustCodeGraph Methods

The TypeScript `RustCodeGraph` class exposes 65 public methods. The Rust
`RustCodeGraph` facade exposes the same method set with idiomatic snake_case names:

| TypeScript | Rust |
|---|---|
| `init` | `init` |
| `initSync` | `init_sync` |
| `open` | `open` |
| `openSync` | `open_sync` |
| `isInitialized` | `is_initialized` |
| `close` | `close` |
| `getProjectRoot` | `get_project_root` |
| `indexAll` | `index_all` |
| `indexFiles` | `index_files` |
| `sync` | `sync` |
| `isIndexing` | `is_indexing` |
| `watch` | `watch` |
| `unwatch` | `unwatch` |
| `isWatching` | `is_watching` |
| `isWatcherDegraded` | `is_watcher_degraded` |
| `getWatcherDegradedReason` | `get_watcher_degraded_reason` |
| `getPendingFiles` | `get_pending_files` |
| `waitUntilWatcherReady` | `wait_until_watcher_ready` |
| `getChangedFiles` | `get_changed_files` |
| `getLastIndexedAt` | `get_last_indexed_at` |
| `getIndexBuildInfo` | `get_index_build_info` |
| `isIndexStale` | `is_index_stale` |
| `extractFromSource` | `extract_from_source` |
| `resolveReferences` | `resolve_references` |
| `resolveReferencesBatched` | `resolve_references_batched` |
| `getDetectedFrameworks` | `get_detected_frameworks` |
| `reinitializeResolver` | `reinitialize_resolver` |
| `getStats` | `get_stats` |
| `getBackend` | `get_backend` |
| `getJournalMode` | `get_journal_mode` |
| `getNode` | `get_node` |
| `getNodesInFile` | `get_nodes_in_file` |
| `getNodesByKind` | `get_nodes_by_kind` |
| `getNodesByName` | `get_nodes_by_name` |
| `searchNodes` | `search_nodes` |
| `getProjectNameTokens` | `get_project_name_tokens` |
| `getTopRouteFile` | `get_top_route_file` |
| `getRoutingManifest` | `get_routing_manifest` |
| `getOutgoingEdges` | `get_outgoing_edges` |
| `getIncomingEdges` | `get_incoming_edges` |
| `getFile` | `get_file` |
| `getFiles` | `get_files` |
| `getContext` | `get_context` |
| `traverse` | `traverse` |
| `getCallGraph` | `get_call_graph` |
| `getTypeHierarchy` | `get_type_hierarchy` |
| `findUsages` | `find_usages` |
| `getCallers` | `get_callers` |
| `getCallees` | `get_callees` |
| `getImpactRadius` | `get_impact_radius` |
| `findPath` | `find_path` |
| `getAncestors` | `get_ancestors` |
| `getChildren` | `get_children` |
| `getFileDependencies` | `get_file_dependencies` |
| `getFileDependents` | `get_file_dependents` |
| `findCircularDependencies` | `find_circular_dependencies` |
| `findDeadCode` | `find_dead_code` |
| `getNodeMetrics` | `get_node_metrics` |
| `getCode` | `get_code` |
| `findRelevantContext` | `find_relevant_context` |
| `buildContext` | `build_context` |
| `optimize` | `optimize` |
| `clear` | `clear` |
| `destroy` | `destroy` |
| `uninitialize` | `uninitialize` |

## Crate-Root Exports

The Rust crate root re-exports the stable SDK building blocks that TypeScript
exports from its package entry:

- Core graph data types from `types`.
- Database access: `DatabaseConnection`, `DatabaseBackend`, `QueryBuilder`,
  and `get_database_path`.
- Directory helpers: `RUSTCODEGRAPH_DIR`, `get_code_graph_dir`,
  `find_nearest_code_graph_root`, and `is_initialized`.
- Grammar helpers: `detect_language`, `is_language_supported`,
  `is_grammar_loaded`, `get_supported_languages`, `init_grammars`,
  `load_grammars_for_languages`, and `load_all_grammars`.
- Resolution result typing: `ResolutionResult`.
- Utility types/functions: `Mutex`, `FileLock`, `process_in_batches`,
  `debounce`, `throttle`, and `MemoryMonitor`.
- MCP server entry: `MCPServer`.
- Watcher entry: `FileWatcher`, `LockUnavailableError`,
  `FileWatcherOptions`, and `FileWatcherPendingFile`.

## Intentional Differences

- Rust uses snake_case names instead of camelCase. This is a language-level API
  convention, not a missing method.
- Rust constructors return `Result<RustCodeGraph, CodeGraphError>` instead of
  throwing exceptions. Methods that are async in TypeScript are synchronous in
  the current Rust facade unless their underlying Rust helper is explicitly
  async, such as grammar loading.
- `RUSTCODEGRAPH_DIR` is the only runtime directory environment variable.
  `CODEGRAPH_DIR` is intentionally not supported by RustCodeGraph.
- `RustCodeGraph::watch` uses a lightweight facade `WatchOptions` and returns
  lightweight facade `PendingFile` values. Direct `FileWatcher` construction
  uses `FileWatcherOptions` and `FileWatcherPendingFile` because the low-level
  watcher options include callbacks that are not serde-friendly facade data.
- `IndexOptions` does not expose TypeScript's `AbortSignal` or callback-shaped
  progress hooks. Cancellation/progress callbacks belong to the async runtime
  parity work, not to the public surface audit.
- `index_files` is present for API parity, but the Rust facade still
  delegates to the current full-index path until incremental re-resolution can
  update cross-file edges without dropping incoming relationships. That
  behavioral parity belongs to the sync/indexer parity task.
- The TypeScript default export has no Rust equivalent. Rust users import
  `rustcodegraph::RustCodeGraph`.

## Required / Obsolete / Different

- Required and covered: all 65 `RustCodeGraph` methods and the stable root exports
  listed above.
- Obsolete in Rust: TypeScript's `default` export.
- Intentionally different: naming, error handling, callback options, direct
  watcher option types, and the temporary `index_files` full-index behavior.

The public surface tests in `tests/foundation_test.rs` compile against the crate
root exports and call the reference-resolution facade methods so accidental API
removal is caught by normal Rust test runs.
