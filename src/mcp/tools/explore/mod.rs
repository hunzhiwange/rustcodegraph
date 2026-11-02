//! Implementation of `rustcodegraph_explore`.
//!
//! explore 是主入口：一次调用要同时给出 flow、blast radius 和按文件组织的
//! Read-equivalent 源码，因此预算、排序和动态边界提示都在这里汇合。

mod dynamic;
mod flow;
mod render;

use std::collections::HashSet;
use std::path::Path;

use serde_json::Value;

use super::budget::{adaptive_explore_enabled, get_explore_budget, get_explore_output_budget};
use super::response::{ToolResult, text_result, truncate_output_to_limit};
use super::validation::{not_indexed_message, unindexed_project_path_message};
use super::{ExploreOutputBudget, ToolHandler};
use crate::types::Node;

pub(super) fn run(
    handler: &mut ToolHandler,
    args: &serde_json::Map<String, Value>,
    project_path: Option<&str>,
    project_root: Option<&str>,
    session: Option<&crate::mcp::explore_session_state::ExploreSessionState>,
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
    let file_count = cg.get_stats().file_count as usize;
    let budget = get_explore_output_budget(file_count);
    // maxFiles 可由调用方收紧，但默认值随 repo size 变化；输出最终仍受
    // max_output_chars 保护。
    let max_files = args
        .get("maxFiles")
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .unwrap_or(budget.default_max_files);
    let mut seen = HashSet::new();
    let mut nodes = Vec::new();
    for token in query.split_whitespace() {
        let token = token
            .trim_matches(|ch: char| !ch.is_alphanumeric() && ch != '.' && ch != '_' && ch != '-');
        if token.is_empty() {
            continue;
        }
        for result in cg.search_nodes(token, None) {
            let node = result.node;
            if seen.insert(node.id.clone()) {
                nodes.push(node);
            }
        }
    }

    let result = if nodes.is_empty() {
        text_result(&format!("No results found for `{query}`"))
    } else {
        let mut output = format_explore_file_result(
            &mut cg,
            Path::new(project_root),
            query,
            nodes,
            max_files,
            &budget,
            file_count,
            session,
        );
        if budget.include_budget_note {
            output.push_str(&format!(
                "\n\n> **Explore budget: {} calls for this project ({} files indexed).** If your question spans more than this response, spend remaining calls on another rustcodegraph_explore before falling back to other tools.",
                get_explore_budget(file_count),
                file_count
            ));
        }
        text_result(&truncate_output_to_limit(&output, budget.max_output_chars))
    };
    cg.close();

    let result = handler.with_index_state_notice(result, project_root, &HashSet::new());
    handler.with_worktree_notice(result, project_path)
}

pub(super) fn format_explore_file_result(
    cg: &mut crate::CodeGraph,
    project_root: &Path,
    query: &str,
    mut nodes: Vec<Node>,
    max_files: usize,
    budget: &ExploreOutputBudget,
    _file_count: usize,
    session: Option<&crate::mcp::explore_session_state::ExploreSessionState>,
) -> String {
    nodes.sort_by(|a, b| {
        a.file_path
            .cmp(&b.file_path)
            .then_with(|| a.start_line.cmp(&b.start_line))
            .then_with(|| a.name.cmp(&b.name))
    });

    let query_tokens = render::explore_query_tokens(query);
    let named_nodes = flow::exact_named_query_nodes(&nodes, &query_tokens);
    let protected = flow::protected_flow_nodes(cg, &named_nodes);
    let protected_files: HashSet<_> = protected.iter().map(|n| n.file_path.clone()).collect();
    nodes.extend(
        protected
            .iter()
            .filter(|n| !nodes.iter().any(|old| old.id == n.id))
            .cloned()
            .collect::<Vec<_>>(),
    );
    let mut source_tokens = query_tokens.clone();
    source_tokens.extend(protected.iter().map(|n| n.qualified_name.clone()));
    let unique_callable_files = render::unique_named_callable_files(cg, &query_tokens);
    let family = render::explore_family_context(cg);
    let adaptive = adaptive_explore_enabled();
    let mut file_paths = Vec::new();
    let mut seen_files = HashSet::new();
    for node in &nodes {
        if seen_files.insert(node.file_path.clone()) {
            file_paths.push(node.file_path.clone());
        }
    }
    // Rank by matched symbol mass; exact named flow endpoints receive extra weight.
    let weights: std::collections::HashMap<String, usize> = file_paths
        .iter()
        .map(|path| {
            let file_named = flow::exact_named_query_nodes(
                &nodes
                    .iter()
                    .filter(|n| &n.file_path == path)
                    .cloned()
                    .collect::<Vec<_>>(),
                &query_tokens,
            );
            let flow_weight = if flow::has_static_flow_between_named(cg, &file_named) {
                2
            } else {
                1
            };
            let weight = nodes
                .iter()
                .filter(|n| &n.file_path == path)
                .map(|n| if query_tokens.contains(&n.name) { 2 } else { 1 })
                .sum::<usize>();
            (path.clone(), weight * flow_weight)
        })
        .collect();
    file_paths.sort_by(|a, b| {
        protected_files
            .contains(b)
            .cmp(&protected_files.contains(a))
            .then_with(|| weights[b].cmp(&weights[a]))
            .then_with(|| a.cmp(b))
    });
    // Weak relative evidence becomes a location pointer, not a source slot.
    // Cap the cliff so a concentrated query cannot silence a strong peer.
    let cliff = (weights.values().copied().max().unwrap_or(0) as f64 * 0.15).min(4.0);
    let mut pointer_files = Vec::new();
    file_paths.retain(|path| {
        if !protected_files.contains(path) && (weights[path] as f64) < cliff {
            pointer_files.push(path.clone());
            false
        } else {
            true
        }
    });
    if max_files > 0 && file_paths.len() > max_files {
        file_paths.truncate(max_files);
    }

    let mut lines = vec![
        format!("## Explore Results ({})", nodes.len()),
        String::new(),
    ];
    let named_nodes = flow::exact_named_query_nodes(&nodes, &query_tokens);
    lines.extend(flow::format_flow_section(cg, &named_nodes, &query_tokens));
    let dynamic_links = dynamic::format_dynamic_dispatch_links(cg, project_root, &named_nodes);
    let has_dynamic_links = !dynamic_links.is_empty();
    if has_dynamic_links {
        lines.push(String::new());
        lines.extend(dynamic_links);
    }
    if !has_dynamic_links && !flow::has_static_flow_between_named(cg, &named_nodes) {
        // 只有没有已知静态/合成路径时才展示 dynamic boundary，避免在已有路径
        // 的答案里增加“也许动态”的噪声。
        let dynamic_boundaries =
            dynamic::format_dynamic_boundaries(cg, project_root, &named_nodes, &query_tokens);
        if !dynamic_boundaries.is_empty() {
            lines.push(String::new());
            lines.extend(dynamic_boundaries);
        }
    }
    let blast_radius = flow::format_explore_blast_radius(cg, &nodes);
    if !blast_radius.is_empty() {
        lines.push(String::new());
        lines.extend(blast_radius);
    }
    lines.push(String::new());
    if !pointer_files.is_empty() {
        lines.push("### Additional symbol locations (source not included)".to_string());
        for path in pointer_files {
            for node in nodes.iter().filter(|node| node.file_path == path) {
                lines.push(format!("- {}:{} — {}", path, node.start_line, node.name));
            }
        }
        lines.push(String::new());
    }
    lines.push("### Source Code".to_string());

    let reservations = super::budget::allocate_explore_source_budget(
        &file_paths.iter().map(|p| weights[p]).collect::<Vec<_>>(),
        budget.max_output_chars / 2,
    );
    let mut reclaimed = 0usize;
    let mut fallback = None;
    let mut has_source = false;
    for (file_path, reservation) in file_paths.into_iter().zip(reservations) {
        let mode = render::explore_file_mode(
            &file_path,
            adaptive,
            &query_tokens,
            &unique_callable_files,
            &family,
        );
        lines.push(String::new());
        let original = render::render_explore_file_section(
            project_root,
            &file_path,
            &cg.get_nodes_in_file(&file_path),
            mode,
            &source_tokens,
            &family,
            reservation.saturating_add(reclaimed).max(1),
        )
        .join("\n")
            + "\n";
        let emitted = session
            .map(|s| s.deduplicate(project_root, &original))
            .unwrap_or_else(|| original.clone());
        reclaimed = reservation
            .saturating_add(reclaimed)
            .saturating_sub(emitted.len());
        if emitted != original && fallback.is_none() {
            fallback = Some((lines.len(), original));
        }
        has_source |= emitted.lines().any(|line| {
            line.split_once('\t')
                .is_some_and(|(n, _)| n.parse::<usize>().is_ok())
        });
        lines.push(emitted);
    }

    if !has_source {
        if let Some((index, source)) = fallback {
            lines[index] = source;
        }
    }
    lines.join("\n")
}
