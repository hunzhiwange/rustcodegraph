//! Implementation of `rustcodegraph_node`.
//!
//! node 是 explore 之后的深挖工具：传 symbol 返回定义源码和 caller/callee trail；
//! 传 file 则走 file_view，作为 indexed source 的 Read 替代。

mod file_view;
mod symbol;

use std::collections::HashSet;
use std::path::Path;

use serde_json::Value;

use super::ToolHandler;
use super::response::{ToolResult, text_result, truncate_output};
use super::shared::{find_symbol_matches, normalize_slashes};
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

    let symbol = args
        .get("symbol")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default();
    let include_code = args
        .get("includeCode")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let file_hint = args.get("file").and_then(Value::as_str).map(str::trim);
    let symbols_only = args
        .get("symbolsOnly")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let offset = args
        .get("offset")
        .and_then(Value::as_u64)
        .unwrap_or(1)
        .max(1) as usize;
    let limit = args
        .get("limit")
        .and_then(Value::as_u64)
        .map(|limit| limit.max(1) as usize);

    if symbol.is_empty() {
        let output = match file_hint.filter(|hint| !hint.is_empty()) {
            Some(file_hint) => file_view::node_file_view_result(
                &mut cg,
                Path::new(project_root),
                file_hint,
                symbols_only,
                offset,
                limit,
            ),
            None => "Provide `symbol` for a symbol lookup or `file` for a file view.".to_string(),
        };
        cg.close();
        let result =
            handler.with_index_state_notice(text_result(&output), project_root, &HashSet::new());
        return handler.with_worktree_notice(result, project_path);
    }

    let mut nodes = find_symbol_matches(&mut cg, symbol);
    if let Some(file_hint) = file_hint.filter(|hint| !hint.is_empty()) {
        let normalized_hint = normalize_slashes(file_hint);
        nodes.retain(|node| {
            let normalized_path = normalize_slashes(&node.file_path);
            normalized_path == normalized_hint
                || normalized_path.ends_with(&format!("/{normalized_hint}"))
                || normalized_path.ends_with(&normalized_hint)
        });
    }
    if let Some(line) = args.get("line").and_then(Value::as_u64) {
        ToolHandler::narrow_nodes_to_line(&mut nodes, line);
    }

    let result = if nodes.is_empty() {
        text_result(&format!("No symbol found for `{symbol}`"))
    } else {
        // 同名 overload/多包定义一次性全部返回，避免 agent 先 Read 文件再判断
        // 哪个定义才是目标。
        nodes.sort_by(|a, b| {
            a.file_path
                .cmp(&b.file_path)
                .then_with(|| a.start_line.cmp(&b.start_line))
                .then_with(|| a.name.cmp(&b.name))
        });
        let mut lines = if nodes.len() > 1 && nodes.iter().all(|node| node.name == symbol) {
            vec![format!(
                "Found {} definitions named \"{symbol}\":",
                nodes.len()
            )]
        } else {
            vec![format!(
                "Found {} definition(s) for `{symbol}`:",
                nodes.len()
            )]
        };
        for node in nodes {
            symbol::push_node_summary(
                &mut lines,
                &mut cg,
                &node,
                include_code,
                Some(Path::new(project_root)),
            );
        }
        text_result(&truncate_output(&lines.join("\n")))
    };
    cg.close();

    let result = handler.with_index_state_notice(result, project_root, &HashSet::new());
    handler.with_worktree_notice(result, project_path)
}
