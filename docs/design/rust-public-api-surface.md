# Rust 公共 API 表面

更新日期：2026-06-21

此审核将 `src/index.ts` 中的 TypeScript 包入口点与
`src/index.rs` 中的 Rust 箱入口点。

## RustCodeGraph 方法

TypeScript `RustCodeGraph` 类公开了 65 个公共方法。铁锈
`RustCodeGraph` 外观公开了具有惯用的 Snake_case 名称的相同方法集：

| 打字稿 | 锈 |
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

## 板条箱根导出

Rust 箱根重新导出了 TypeScript 所使用的稳定 SDK 构建块
从其包条目导出：

- `types` 的核心图数据类型。
- 数据库访问：`DatabaseConnection`、`DatabaseBackend`、`QueryBuilder`、
和 `get_database_path`。
- 目录助手：`RUSTCODEGRAPH_DIR`、`get_code_graph_dir`、
`find_nearest_code_graph_root` 和 `is_initialized`。
- 语法助手：`detect_language`、`is_language_supported`、
`is_grammar_loaded`、`get_supported_languages`、`init_grammars`、
`load_grammars_for_languages` 和 `load_all_grammars`。
- 分辨率结果输入：`ResolutionResult`。
- 实用程序类型/功能：`Mutex`、`FileLock`、`process_in_batches`、
`debounce`、`throttle` 和 `MemoryMonitor`。
- MCP 服务器条目：`MCPServer`。
- 观察者条目：`FileWatcher`、`LockUnavailableError`、
`FileWatcherOptions` 和 `FileWatcherPendingFile`。

## 故意的差异

- Rust 使用蛇形命名法而不是驼峰命名法。这是一个语言级的API
约定，而不是缺少方法。
- Rust 构造函数返回 `Result<RustCodeGraph, CodeGraphError>` 而不是
抛出异常。 TypeScript 中的异步方法在 TypeScript 中是同步的
当前的 Rust 外观，除非它们的底层 Rust 助手被显式地声明
异步，例如语法加载。
- `RUSTCODEGRAPH_DIR` 是唯一的运行时目录环境变量。
RustCodeGraph 故意不支持 `CODEGRAPH_DIR`。
- `RustCodeGraph::watch` 使用轻量级立面 `WatchOptions` 并返回
轻质立面 `PendingFile` 值。直接`FileWatcher`施工
使用 `FileWatcherOptions` 和 `FileWatcherPendingFile` 因为低级
观察者选项包括不支持 serde 的外观数据的回调。
- `IndexOptions` 不公开 TypeScript 的 `AbortSignal` 或回调形状
进度挂钩。取消/进度回调属于异步运行时
比价工作，不向公众表面审计。
- `index_files` 存在用于 API 奇偶校验，但 Rust 外观仍然存在
委托到当前的全索引路径，直到增量重新解析可以
更新跨文件边缘而不丢失传入关系。那
行为奇偶校验属于同步/索引器奇偶校验任务。
- TypeScript 默认导出没有 Rust 等效项。 Rust 用户导入
`rustcodegraph::RustCodeGraph`。

## 必需/过时/不同

- 必需并涵盖：所有 65 个 `RustCodeGraph` 方法和稳定根导出
上面列出了。
- Rust 中已过时：TypeScript 的 `default` 导出。
- 故意不同：命名、错误处理、回调选项、直接
观察者选项类型，以及临时 `index_files` 全索引行为。

`tests/foundation_test.rs` 中的公共表面测试针对 crate 进行编译
root 导出并调用引用解析外观方法，因此意外 API
正常的 Rust 测试运行会捕获删除。
