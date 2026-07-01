//! Public CodeGraph facade.
//!
//! This is the Rust counterpart to `src/index.ts`. The `CodeGraph` methods map
//! to the TypeScript entry point with idiomatic snake_case names, and the crate
//! root re-exports the same SDK building blocks where Rust has a stable
//! equivalent. A few runtime-shaped TypeScript exports intentionally remain
//! module-qualified or aliased; see `docs/design/rust-public-api-surface.md`.
//!
//! facade 层把数据库、抽取、resolver、watcher 和 MCP 需要的查询聚合成一个
//! Rust SDK 入口。子模块按职责拆分，避免这个 public API 文件继续膨胀。

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc, Mutex as StdMutex,
    atomic::{AtomicBool, Ordering},
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, params, params_from_iter};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub use crate::db::index::{DatabaseBackend, DatabaseConnection, get_database_path};
pub use crate::db::queries::QueryBuilder;
pub use crate::directory::{
    RUSTCODEGRAPH_DIR, find_nearest_code_graph_root, get_code_graph_dir, is_initialized,
};
pub use crate::errors::{
    CodeGraphError, ConfigError, DEFAULT_LOGGER, DatabaseError, FileError, Logger, ParseError,
    SILENT_LOGGER, SearchError, VectorError, get_logger, set_logger,
};
pub use crate::extraction::grammars::{
    detect_language, get_supported_languages, init_grammars, is_grammar_loaded,
    is_language_supported, load_all_grammars, load_grammars_for_languages,
};
pub use crate::mcp::index::MCPServer;
pub use crate::resolution::types::ResolutionResult;
pub use crate::sync::watcher::{
    FileWatcher, LockUnavailableError, PendingFile as FileWatcherPendingFile,
    WatchOptions as FileWatcherOptions,
};
pub use crate::types::*;
pub use crate::utils::{FileLock, MemoryMonitor, Mutex, debounce, process_in_batches, throttle};
#[allow(deprecated)]
pub use crate::utils::{current_watch_memory_usage_bytes, set_watch_memory_reader_for_tests};

use crate::directory::{create_directory, remove_directory, validate_directory};
use crate::extraction::extraction_version::EXTRACTION_VERSION;
use crate::extraction::grammars::language_key;
use crate::extraction::index::{hash_content, scan_directory};
use crate::extraction::tree_sitter::extract_from_source as extract_source_now;
use crate::resolution::callback_synthesizer::synthesize_callback_edges;
use crate::resolution::frameworks::index::{ResolverRef, get_all_framework_resolvers};
use crate::resolution::import_resolver::extract_import_mappings;
use crate::resolution::name_matcher::{crosses_known_family, same_language_family};
use crate::resolution::types::{ImportMapping, ReExport, ResolutionContext};
use crate::sync::watcher::{
    FileWatcher as RuntimeFileWatcher, SyncRunResult, WatchOptions as RuntimeWatchOptions,
    facade_degraded_reason, facade_pending_files, register_facade_runtime_watcher,
    unregister_facade_watcher, update_facade_runtime_watcher,
};

#[derive(Debug, Clone, Default)]
pub struct InitOptions {
    pub index: bool,
}

#[derive(Debug, Clone, Default)]
pub struct OpenOptions {
    pub sync: bool,
    pub read_only: bool,
}

#[derive(Debug, Clone, Default)]
pub struct IndexOptions {
    pub verbose: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexProgress {
    pub phase: String,
    pub current: usize,
    pub total: usize,
    pub current_file: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexResult {
    pub success: bool,
    pub files_indexed: usize,
    pub files_skipped: usize,
    pub files_errored: usize,
    pub nodes_created: usize,
    pub edges_created: usize,
    pub errors: Vec<ExtractionError>,
    pub duration_ms: u64,
}

impl Default for IndexResult {
    fn default() -> Self {
        Self {
            success: true,
            files_indexed: 0,
            files_skipped: 0,
            files_errored: 0,
            nodes_created: 0,
            edges_created: 0,
            errors: Vec::new(),
            duration_ms: 0,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncResult {
    pub files_checked: usize,
    pub files_added: usize,
    pub files_modified: usize,
    pub files_removed: usize,
    pub nodes_updated: usize,
    pub duration_ms: u64,
    pub changed_file_paths: Option<Vec<String>>,
    /// Compatibility field retained for older callers.
    ///
    /// RustCodeGraph's built-in sync path no longer skips work based on process
    /// memory readings, so this remains `false` for normal `sync` results.
    pub memory_skipped: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChangedFiles {
    pub added: Vec<String>,
    pub modified: Vec<String>,
    pub removed: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchOptions {
    pub debounce_ms: Option<u64>,
    pub max_debounce_ms: Option<u64>,
    pub min_sync_interval_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingFile {
    pub path: String,
    pub first_seen_ms: i64,
    pub last_seen_ms: i64,
    pub indexing: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexBuildInfo {
    pub version: Option<String>,
    pub extraction_version: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TopRouteFile {
    pub file_path: String,
    pub route_count: u64,
    pub total_routes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutingManifestEntry {
    pub url: String,
    pub handler: String,
    pub handler_file: String,
    pub handler_line: u64,
    pub handler_kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutingManifest {
    pub entries: Vec<RoutingManifestEntry>,
    pub top_handler_file: Option<String>,
    pub top_handler_file_count: u64,
    pub total_routes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeMetrics {
    pub incoming_edge_count: u64,
    pub outgoing_edge_count: u64,
    pub call_count: u64,
    pub caller_count: u64,
    pub child_count: u64,
    pub depth: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PathStep {
    pub node: Node,
    pub edge: Option<Edge>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BuildContextResult {
    Context(Box<TaskContext>),
    Formatted(String),
}

pub struct CodeGraph {
    project_root: PathBuf,
    db: DatabaseConnection,
    // 这些运行态字段只属于 facade 实例，不写入 project metadata；watcher 状态
    // 需要和全局 watcher registry 同步，生命周期方法负责维护它们。
    indexing: bool,
    watching: bool,
    watch_registered: bool,
    watch_stop: Option<Arc<AtomicBool>>,
    watch_thread: Option<JoinHandle<()>>,
    watcher: Option<Arc<StdMutex<RuntimeFileWatcher>>>,
    watcher_degraded_reason: Option<String>,
    pending_files: Vec<PendingFile>,
    index_build_info: IndexBuildInfo,
}

mod class_members;
mod context_methods;
mod database;
mod edge_resolution;
mod extract_rich;
mod facade_helpers;
mod fn_ref_expr;
mod frameworks;
mod function_refs;
mod graph_methods;
mod index_methods;
mod indexing;
mod inline_class;
mod language_extras;
mod lifecycle_methods;
mod line_utils;
mod member_resolution;
mod node_builders;
mod pending;
mod php_ruby_refs;
mod post_index;
mod property_edges;
mod query_methods;
mod relations;
mod scoped_resolution;
mod stats_methods;
mod status_methods;
mod syncing;
mod syntax_utils;
mod test_edges;
mod value_edges;
mod watch_methods;

pub use crate::utils::debug_rss as debug_rss_pub;
use class_members::*;
use database::*;
use edge_resolution::*;
use extract_rich::*;
use facade_helpers::*;
use fn_ref_expr::*;
use frameworks::*;
use function_refs::*;
use indexing::*;
use inline_class::*;
use language_extras::*;
use line_utils::*;
use member_resolution::*;
use node_builders::*;
use pending::*;
use php_ruby_refs::*;
pub use post_index::facade_synthesis_nodes_loaded;
use post_index::*;
use property_edges::*;
use relations::*;
use scoped_resolution::*;
pub use syncing::facade_file_content_reads;
#[allow(deprecated)]
pub use syncing::facade_watch_memory_skips;
use syncing::*;
use syntax_utils::*;
use test_edges::*;
use value_edges::*;
