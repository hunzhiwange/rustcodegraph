//! Symbol rendering for `rustcodegraph_node`.
//!
//! 这里把单个 Node 渲染成位置、可选源码和一跳调用 trail。它不负责查找符号，
//! 只消费上层已经选出的候选 Node。

use std::fs;
use std::path::Path;

use super::super::graph_query::callees_for_definition;
use super::super::shared::{
    dedupe_node_edge_rows, edge_kind_label, node_kind_label, project_file_path, sort_node_edge_rows,
};
use super::file_view::numbered_lines;
use crate::types::{Edge, Language, Node, NodeKind};

fn is_config_leaf(node: &Node) -> bool {
    node.kind == NodeKind::Constant
        && matches!(node.language, Language::Yaml | Language::Properties)
}

fn node_source_snippet(project_root: &Path, node: &Node) -> Option<String> {
    let source = fs::read_to_string(project_file_path(project_root, &node.file_path)).ok()?;
    let source_lines = source.lines().collect::<Vec<_>>();
    if source_lines.is_empty() {
        return None;
    }
    let start = usize::try_from(node.start_line).ok()?.max(1);
    let mut end = usize::try_from(node.end_line)
        .ok()
        .unwrap_or(start)
        .max(start)
        .min(source_lines.len());
    if start > source_lines.len() {
        return None;
    }
    if end == start {
        end = infer_block_end_line(&source_lines, start);
    }
    Some(numbered_lines(&source_lines, start, end).join("\n"))
}

fn infer_block_end_line(source_lines: &[&str], start: usize) -> usize {
    // 某些抽取器只给出起始行；最多向后探 80 行并用大括号平衡推断函数/类块，
    // 防止 includeCode 只返回签名行。
    let start_index = start.saturating_sub(1);
    let mut balance = 0i32;
    let mut saw_open = false;
    let max_index = (start_index + 80).min(source_lines.len().saturating_sub(1));
    for (index, line) in source_lines
        .iter()
        .enumerate()
        .take(max_index + 1)
        .skip(start_index)
    {
        for ch in line.chars() {
            match ch {
                '{' => {
                    saw_open = true;
                    balance += 1;
                }
                '}' => balance -= 1,
                _ => {}
            }
        }
        if saw_open && balance <= 0 {
            return index + 1;
        }
    }
    start
}

pub(super) fn push_node_summary(
    lines: &mut Vec<String>,
    cg: &mut crate::CodeGraph,
    node: &Node,
    include_code: bool,
    project_root: Option<&Path>,
) {
    let display_name = if node.qualified_name.is_empty() {
        node.name.as_str()
    } else {
        node.qualified_name.as_str()
    };
    lines.push(format!("## {} ({})", node.name, node_kind_label(node.kind)));
    lines.push(format!(
        "**Location:** {}:{}",
        node.file_path, node.start_line
    ));
    lines.push(format!(
        "- {} `{}` at {}:{}",
        node_kind_label(node.kind),
        display_name,
        node.file_path,
        node.start_line
    ));

    if is_config_leaf(node) {
        lines.push(format!(
            "  Config key `{display_name}` is indexed; config values are withheld."
        ));
    } else if include_code {
        if let Some(snippet) = project_root.and_then(|root| node_source_snippet(root, node)) {
            lines.push("### Source".to_string());
            lines.push(snippet);
        } else {
            lines.push("  Source output is not available from this Rust MCP path yet.".to_string());
        }
    }

    let trail = format_node_trail(cg, node);
    if !trail.is_empty() {
        lines.extend(trail);
    }
}

fn format_node_trail(cg: &mut crate::CodeGraph, node: &Node) -> Vec<String> {
    // trail 是“下一步该查谁”的导航，不追求完整图遍历；完整 callers/impact
    // 由 graph_query 工具承担。
    let mut callees = callees_for_definition(cg, node, 1);
    let mut callers = cg
        .get_callers(&node.id, 1)
        .into_iter()
        .map(|entry| (entry.node, entry.edge))
        .collect::<Vec<_>>();
    sort_node_edge_rows(&mut callees);
    sort_node_edge_rows(&mut callers);
    dedupe_node_edge_rows(&mut callees);
    dedupe_node_edge_rows(&mut callers);

    if callees.is_empty() && callers.is_empty() {
        return Vec::new();
    }

    const TRAIL_CAP: usize = 12;
    let mut lines = vec![
        String::new(),
        "### Trail - rustcodegraph_node any of these to follow it (no Read needed)".to_string(),
    ];
    if !callees.is_empty() {
        lines.push(format!(
            "**Calls ->** {}{}",
            format_trail_entries(&callees, TRAIL_CAP),
            more_suffix(callees.len(), TRAIL_CAP)
        ));
    }
    if !callers.is_empty() {
        lines.push(format!(
            "**Called by <-** {}{}",
            format_trail_entries(&callers, TRAIL_CAP),
            more_suffix(callers.len(), TRAIL_CAP)
        ));
    }
    lines
}

fn format_trail_entries(rows: &[(Node, Edge)], cap: usize) -> String {
    rows.iter()
        .take(cap)
        .map(|(node, edge)| {
            format!(
                "{} ({}:{}) via {}",
                node.name,
                node.file_path,
                node.start_line,
                edge_kind_label(edge.kind)
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn more_suffix(len: usize, cap: usize) -> String {
    if len > cap {
        format!(", +{} more", len - cap)
    } else {
        String::new()
    }
}
