//! Graph-query MCP tools: callers, callees, and impact.
//!
//! 这些工具以“一个符号名可能对应多个定义”为默认模型，宁可分段展示所有
//! overload/monorepo 同名定义，也不把不相关调用点合并到一起。

use std::collections::HashSet;
use std::fs;
use std::path::Path;

use regex::Regex;
use serde_json::Value;

use super::ToolHandler;
use super::response::{ToolResult, text_result, truncate_output};
use super::shared::{
    dedupe_node_edge_rows, dedupe_nodes, edge_kind_label, file_hint_matches, node_kind_label,
    normalize_slashes, project_file_path, same_monorepo_scope, sort_node_edge_rows,
    sort_nodes_for_output,
};
use super::validation::{not_indexed_message, unindexed_project_path_message};
use crate::types::{EdgeKind, Node, NodeKind};

pub(super) fn run(
    handler: &mut ToolHandler,
    tool_name: &str,
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
    let file_hint = args.get("file").and_then(Value::as_str).map(str::trim);
    let limit = args
        .get("limit")
        .and_then(Value::as_u64)
        .unwrap_or(20)
        .clamp(1, 100) as usize;
    let depth = args
        .get("depth")
        .and_then(Value::as_u64)
        .unwrap_or(2)
        .clamp(1, 8);
    let selection = select_symbol_definitions(&mut cg, symbol, file_hint);

    let output = if selection.nodes.is_empty() {
        format!("No definition found for `{symbol}`")
    } else {
        match tool_name {
            "rustcodegraph_callers" => format_callers_output(
                &mut cg,
                Path::new(project_root),
                symbol,
                &selection,
                1,
                limit,
            ),
            "rustcodegraph_callees" => format_callees_output(&mut cg, symbol, &selection, 1, limit),
            "rustcodegraph_impact" => format_impact_output(&mut cg, symbol, &selection, depth),
            _ => format!("Unknown graph query tool: {tool_name}"),
        }
    };
    cg.close();

    let result = handler.with_index_state_notice(
        text_result(&truncate_output(&output)),
        project_root,
        &HashSet::new(),
    );
    handler.with_worktree_notice(result, project_path)
}

struct SymbolSelection {
    nodes: Vec<Node>,
    note: Option<String>,
}

fn select_symbol_definitions(
    cg: &mut crate::CodeGraph,
    symbol: &str,
    file_hint: Option<&str>,
) -> SymbolSelection {
    // 先精确 name，再退到搜索；file hint 只做收窄，收窄失败时保留全部定义
    // 并给 note，避免误报“找不到”。
    let mut nodes = cg
        .get_nodes_by_name(symbol)
        .into_iter()
        .filter(is_graph_query_definition)
        .collect::<Vec<_>>();
    if nodes.is_empty() {
        nodes = cg
            .search_nodes(symbol, None)
            .into_iter()
            .map(|result| result.node)
            .filter(is_graph_query_definition)
            .collect();
    }
    sort_nodes_for_output(&mut nodes);
    dedupe_nodes(&mut nodes);

    let Some(file_hint) = file_hint.filter(|hint| !hint.trim().is_empty()) else {
        return SymbolSelection { nodes, note: None };
    };

    let narrowed = nodes
        .iter()
        .filter(|node| file_hint_matches(&node.file_path, file_hint))
        .cloned()
        .collect::<Vec<_>>();
    if !narrowed.is_empty() {
        return SymbolSelection {
            nodes: narrowed,
            note: None,
        };
    }

    SymbolSelection {
        nodes,
        note: Some(format!(
            "no definition of \"{symbol}\" matches file `{}`; showing all definitions.",
            normalize_slashes(file_hint)
        )),
    }
}
fn format_callers_output(
    cg: &mut crate::CodeGraph,
    project_root: &Path,
    symbol: &str,
    selection: &SymbolSelection,
    depth: u64,
    limit: usize,
) -> String {
    let mut lines = graph_query_header("Callers", symbol, selection);
    for node in &selection.nodes {
        if selection.nodes.len() > 1 {
            lines.push(String::new());
            lines.push(definition_heading(node));
        }
        let callers = callers_for_definition(cg, project_root, node, depth);
        if callers.is_empty() {
            lines.push("- No callers found".to_string());
        } else {
            for (caller, edge) in callers.into_iter().take(limit) {
                lines.push(format!(
                    "- {} `{}` at {}:{} via {}",
                    node_kind_label(caller.kind),
                    caller.name,
                    caller.file_path,
                    caller.start_line,
                    edge_kind_label(edge.kind)
                ));
            }
        }
    }
    lines.join("\n")
}
fn format_callees_output(
    cg: &mut crate::CodeGraph,
    symbol: &str,
    selection: &SymbolSelection,
    depth: u64,
    limit: usize,
) -> String {
    let mut lines = graph_query_header("Callees", symbol, selection);
    for node in &selection.nodes {
        if selection.nodes.len() > 1 {
            lines.push(String::new());
            lines.push(definition_heading(node));
        }
        let callees = callees_for_definition(cg, node, depth);
        if callees.is_empty() {
            lines.push("- No callees found".to_string());
        } else {
            for (callee, edge) in callees.into_iter().take(limit) {
                lines.push(format!(
                    "- {} `{}` at {}:{} via {}",
                    node_kind_label(callee.kind),
                    callee.name,
                    callee.file_path,
                    callee.start_line,
                    edge_kind_label(edge.kind)
                ));
            }
        }
    }
    lines.join("\n")
}
fn format_impact_output(
    cg: &mut crate::CodeGraph,
    symbol: &str,
    selection: &SymbolSelection,
    depth: u64,
) -> String {
    let mut lines = graph_query_header("Impact", symbol, selection);
    for node in &selection.nodes {
        if selection.nodes.len() > 1 {
            lines.push(String::new());
            lines.push(definition_heading(node));
        }
        let affected = impact_for_definition(cg, node, depth);
        lines.push(format!("- affects {} symbols", affected.len()));
        for affected in affected.into_iter().take(8) {
            lines.push(format!(
                "  - {} `{}` at {}:{}",
                node_kind_label(affected.kind),
                affected.name,
                affected.file_path,
                affected.start_line
            ));
        }
    }
    lines.join("\n")
}
fn graph_query_header(kind: &str, symbol: &str, selection: &SymbolSelection) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(note) = &selection.note {
        lines.push(note.clone());
        lines.push(String::new());
    }
    if selection.nodes.len() > 1 {
        lines.push(format!(
            "## {kind}: {} distinct definitions named \"{symbol}\"",
            selection.nodes.len()
        ));
    } else if let Some(node) = selection.nodes.first() {
        lines.push(format!(
            "## {kind} for `{symbol}` ({})",
            definition_location(node)
        ));
    } else {
        lines.push(format!("## {kind} for `{symbol}`"));
    }
    lines
}
fn definition_heading(node: &Node) -> String {
    format!(
        "### {} `{}` ({})",
        node_kind_label(node.kind),
        node.name,
        definition_location(node)
    )
}
fn definition_location(node: &Node) -> String {
    format!("{}:{}", node.file_path, node.start_line)
}
fn callers_for_definition(
    cg: &mut crate::CodeGraph,
    project_root: &Path,
    node: &Node,
    depth: u64,
) -> Vec<(Node, crate::types::Edge)> {
    let mut out = cg
        .get_callers(&node.id, depth)
        .into_iter()
        .map(|entry| (entry.node, entry.edge))
        .collect::<Vec<_>>();
    supplement_source_callers(cg, project_root, node, &mut out);
    sort_node_edge_rows(&mut out);
    dedupe_node_edge_rows(&mut out);
    out
}
pub(super) fn callees_for_definition(
    cg: &mut crate::CodeGraph,
    node: &Node,
    depth: u64,
) -> Vec<(Node, crate::types::Edge)> {
    let mut out = cg
        .get_callees(&node.id, depth)
        .into_iter()
        .map(|entry| (entry.node, entry.edge))
        .collect::<Vec<_>>();
    sort_node_edge_rows(&mut out);
    dedupe_node_edge_rows(&mut out);
    out
}
fn impact_for_definition(cg: &mut crate::CodeGraph, node: &Node, depth: u64) -> Vec<Node> {
    let impact = cg.get_impact_radius(&node.id, depth);
    let mut out = impact.nodes.into_values().collect::<Vec<_>>();
    sort_nodes_for_output(&mut out);
    out
}
fn supplement_source_callers(
    cg: &mut crate::CodeGraph,
    project_root: &Path,
    target: &Node,
    out: &mut Vec<(Node, crate::types::Edge)>,
) {
    // 图边可能漏掉简单源码调用（例如解析器尚未覆盖的语法）；这里用同 scope
    // 的函数体文本做保守补充，只作为 callers 输出，不写回 DB。
    if target.name.len() < 3 {
        return;
    }
    let mut candidates = cg.get_nodes_by_kind(NodeKind::Function);
    candidates.extend(cg.get_nodes_by_kind(NodeKind::Method));
    for caller in candidates {
        if caller.id == target.id || !same_monorepo_scope(&caller.file_path, &target.file_path) {
            continue;
        }
        let source = fs::read_to_string(project_file_path(project_root, &caller.file_path))
            .unwrap_or_default();
        let snippet = source_lines_for_node(&source, &caller);
        if !source_mentions_call(&snippet, &target.name) {
            continue;
        }
        out.push((
            caller,
            crate::types::Edge {
                source: String::new(),
                target: target.id.clone(),
                kind: EdgeKind::Calls,
                metadata: None,
                line: Some(target.start_line),
                column: Some(0),
                provenance: None,
            },
        ));
    }
}
fn source_lines_for_node(source: &str, node: &Node) -> String {
    let start = node.start_line.saturating_sub(1) as usize;
    let end = node.end_line.max(node.start_line) as usize;
    source
        .lines()
        .skip(start)
        .take(end.saturating_sub(start).max(1))
        .collect::<Vec<_>>()
        .join("\n")
}
fn source_mentions_call(source: &str, name: &str) -> bool {
    let Ok(re) = Regex::new(&format!(r#"(?:^|[^\w$]){}\s*\("#, regex::escape(name))) else {
        return false;
    };
    re.is_match(source)
}
fn is_graph_query_definition(node: &Node) -> bool {
    matches!(
        node.kind,
        NodeKind::Class
            | NodeKind::Struct
            | NodeKind::Interface
            | NodeKind::Trait
            | NodeKind::Protocol
            | NodeKind::Function
            | NodeKind::Method
            | NodeKind::Component
    )
}
