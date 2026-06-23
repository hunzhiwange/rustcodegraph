//! MCP tool definitions and dispatch facade translated from `tools.ts`.
//!
//! The schemas, budgets, validation limits, and success/error result shapes are
//! preserved here. Query execution is intentionally represented by a deferred
//! backend facade so importing this module never opens SQLite, starts watchers,
//! or registers an MCP server during the translation tasks.
//!
//! 约定：未索引、未找到、文件不在索引内等可恢复状态尽量返回成功形状文本；
//! `isError` 只留给安全拒绝和真正故障，避免 agent 因早期错误放弃工具。

mod budget;
mod explore;
mod files;
mod graph_query;
mod handler;
mod node;
mod registry;
mod response;
mod search;
mod shared;
mod status;
mod validation;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::sync::worktree::WorktreeIndexMismatch;

pub use budget::{
    ExploreOutputBudget, adaptive_explore_enabled, explore_line_numbers_enabled,
    get_explore_budget, get_explore_output_budget, number_source_lines,
};
pub use registry::{get_static_tools, tools};
pub use response::{
    ToolContent, ToolResult, error_result, format_degraded_banner, format_stale_banner,
    format_stale_footer, text_result, truncate_output,
};
pub use shared::glob_to_regex;

pub const MAX_OUTPUT_LENGTH: usize = 15_000;
pub const MAX_INPUT_LENGTH: usize = 10_000;
pub const MAX_PATH_LENGTH: usize = 4_096;
const DEFAULT_NODE_FILE_VIEW_LINES: usize = 200;

const MCP_TOOLS_ENV: &str = "RUSTCODEGRAPH_MCP_TOOLS";
pub const DEFAULT_MCP_TOOLS: &[&str] = &["explore", "node", "search", "callers"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotIndexedError {
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathRefusalError {
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingFile {
    pub path: String,
    pub last_seen_ms: i64,
    pub indexing: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: InputSchema,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InputSchema {
    #[serde(rename = "type")]
    pub schema_type: String,
    pub properties: serde_json::Map<String, Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<Vec<String>>,
}

type CatchUpGate = Arc<Mutex<Option<Box<dyn FnOnce() -> Result<(), String> + Send + 'static>>>>;

#[derive(Clone, Default)]
pub struct ToolHandler {
    // default_project_loaded 决定 tools/list 是否暴露工具；project_root 是真正
    // 打开 CodeGraph 的位置，hint 只用于未索引提示和后续 root 发现。
    default_project_loaded: bool,
    default_project_hint: Option<String>,
    default_project_root: Option<String>,
    worktree_mismatch_cache: HashMap<String, Option<WorktreeIndexMismatch>>,
    file_count: Option<usize>,
    catch_up_gate: Option<CatchUpGate>,
}

impl std::fmt::Debug for ToolHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolHandler")
            .field("default_project_loaded", &self.default_project_loaded)
            .field("default_project_hint", &self.default_project_hint)
            .field("default_project_root", &self.default_project_root)
            .field("worktree_mismatch_cache", &self.worktree_mismatch_cache)
            .field("file_count", &self.file_count)
            .field("catch_up_gate", &self.catch_up_gate.is_some())
            .finish()
    }
}
