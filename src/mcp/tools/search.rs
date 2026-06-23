//! Lightweight implementation of `rustcodegraph_search`.
//!
//! search 只返回位置，不返回源码；它适合快速定位符号，但复杂理解路径应交给
//! explore/node，避免 agent 先拿到一串位置后又回退到 grep/read。

use std::collections::HashSet;
use std::path::Path;

use serde_json::Value;

use super::ToolHandler;
use super::response::{ToolResult, text_result};
use super::shared::{node_kind_filter, node_kind_label, node_matches_query, normalize_slashes};
use super::validation::{not_indexed_message, unindexed_project_path_message};

pub(super) fn run(
    handler: &mut ToolHandler,
    args: &serde_json::Map<String, Value>,
    project_path: Option<&str>,
    project_root: Option<&str>,
) -> ToolResult {
    let Some(project_root) = project_root else {
        return text_result(&not_indexed_message(
            handler.default_project_hint.as_deref(),
        ));
    };
    let Ok(mut cg) = crate::CodeGraph::open_sync(Path::new(project_root)) else {
        return text_result(&unindexed_project_path_message(project_root));
    };

    let query = args
        .get("query")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let needle = query.to_ascii_lowercase();
    let limit = args
        .get("limit")
        .and_then(Value::as_u64)
        .unwrap_or(10)
        .clamp(1, 100) as usize;
    let kind = args
        .get("kind")
        .and_then(Value::as_str)
        .and_then(node_kind_filter);

    let mut nodes = if let Some(kind) = kind {
        // kind 过滤走全量 kind 查询再做轻量匹配；FTS 搜索本身不知道工具层的
        // 简化 kind 别名。
        cg.get_nodes_by_kind(kind)
            .into_iter()
            .filter(|node| node_matches_query(node, &needle))
            .collect::<Vec<_>>()
    } else {
        cg.search_nodes(query, None)
            .into_iter()
            .map(|result| result.node)
            .collect::<Vec<_>>()
    };
    nodes.truncate(limit);
    // 只把实际展示到结果里的文件交给 staleness 检查，避免无关 pending 文件污染
    // 一个定位型工具的输出。
    let referenced_files = nodes
        .iter()
        .map(|node| normalize_slashes(&node.file_path))
        .collect::<HashSet<_>>();
    cg.close();

    let result = if nodes.is_empty() {
        text_result(&format!("No results found for `{query}`"))
    } else {
        let mut lines = Vec::with_capacity(nodes.len() + 2);
        lines.push(format!("Found {} result(s) for `{query}`:", nodes.len()));
        for node in &nodes {
            lines.push(format!(
                "- {} `{}` at {}:{}",
                node_kind_label(node.kind),
                node.name,
                node.file_path,
                node.start_line
            ));
        }
        text_result(&lines.join("\n"))
    };

    let result = handler.with_index_state_notice(result, project_root, &referenced_files);
    handler.with_worktree_notice(result, project_path)
}
