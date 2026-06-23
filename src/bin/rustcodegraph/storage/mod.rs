//! CLI command 模块使用的 SQLite storage facade。
//!
//! 子模块负责 JSON 转换、查询、源码读取、写库和统计；这里集中 re-export 让
//! CLI/MCP handler 不需要知道表结构细节。

mod json_helpers;
mod query;
mod source;
mod sqlite;
mod stats;

use std::collections::BTreeMap;

use rustcodegraph::types::LineNumber;
use serde::Serialize;

pub(crate) use query::{
    affected_files_for_changes, edge_matches_for_symbol, find_symbol_nodes, query_nodes, read_files,
};
pub(crate) use source::{
    format_matches, is_test_file, normalize_index_path, read_node_source, read_numbered_file_range,
};
pub(crate) use sqlite::{
    database_path, initialize_sqlite_database, is_sqlite_initialized, open_sqlite_database,
    write_sqlite_index,
};
pub(crate) use stats::{
    format_mcp_status, read_last_indexed_at, read_sqlite_stats, unix_ms_to_iso,
};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SqliteStats {
    pub(crate) node_count: u64,
    pub(crate) edge_count: u64,
    pub(crate) file_count: u64,
    pub(crate) nodes_by_kind: BTreeMap<String, u64>,
    pub(crate) files_by_language: BTreeMap<String, u64>,
    pub(crate) last_updated: rustcodegraph::types::TimestampMs,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct QueryMatch {
    // 这是 CLI 查询输出的轻量视图，不包含完整 Node 字段，减少 SQLite row mapping 成本。
    pub(crate) id: String,
    pub(crate) kind: String,
    pub(crate) name: String,
    pub(crate) qualified_name: String,
    pub(crate) file_path: String,
    pub(crate) start_line: LineNumber,
    pub(crate) signature: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum EdgeDirection {
    /// 遍历 callers：当前节点是 edge target。
    Incoming,
    /// 遍历 callees：当前节点是 edge source。
    Outgoing,
}
