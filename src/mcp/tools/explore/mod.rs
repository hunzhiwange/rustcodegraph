//! Implementation of `rustcodegraph_explore`.
//!
//! explore 是主入口：一次调用要同时给出 flow、blast radius 和按文件组织的
//! Read-equivalent 源码，因此预算、排序和动态边界提示都在这里汇合。

mod dynamic;
mod flow;
mod render;
mod snippets;

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
    let mut budget = get_explore_output_budget(file_count);
    let response_limit = budget.max_output_chars;
    let budget_note = if budget.include_budget_note {
        format!(
            "\n\n> **Explore budget: {} calls for this project ({} files indexed).** If your question spans more than this response, spend remaining calls on another rustcodegraph_explore before falling back to other tools.",
            get_explore_budget(file_count),
            file_count
        )
    } else {
        String::new()
    };
    budget.max_output_chars = budget.max_output_chars.saturating_sub(budget_note.len());
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
            session,
        );
        output.push_str(&budget_note);
        text_result(&truncate_output_to_limit(&output, response_limit))
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
    let flow_plan = flow::FlowPlan::build(cg, &named_nodes);
    let protected = flow_plan.protected_nodes();
    let mut protected_files: HashSet<_> = protected.iter().map(|n| n.file_path.clone()).collect();
    protected_files.extend(flow_plan.anchors.iter().map(|a| a.file_path.clone()));
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
    for anchor in &flow_plan.anchors {
        if seen_files.insert(anchor.file_path.clone()) {
            file_paths.push(anchor.file_path.clone());
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
            .then_with(|| {
                // Anchor order comes from the selected paths, primary first.
                // Fuzzy match mass must not reorder those evidence files.
                let rank = |path: &str| {
                    flow_plan
                        .anchors
                        .iter()
                        .position(|a| a.file_path == path)
                        .unwrap_or(usize::MAX)
                };
                rank(a).cmp(&rank(b))
            })
            .then_with(|| weights[b].cmp(&weights[a]))
            .then_with(|| a.cmp(b))
    });
    // Weak relative evidence becomes a location pointer, not a source slot.
    // Cap the cliff so a concentrated query cannot silence a strong peer.
    let cliff = (weights.values().copied().max().unwrap_or(0) as f64 * 0.15).min(4.0);
    let mut pointer_files = Vec::new();
    file_paths.retain(|path| {
        // Skeletons are already the compact representation of polymorphic
        // siblings; dropping them here would erase the family signatures.
        let skeleton = matches!(
            render::explore_file_mode(
                path,
                adaptive,
                &query_tokens,
                &unique_callable_files,
                &family,
            ),
            render::ExploreFileMode::Skeleton
        );
        if !protected_files.contains(path) && !skeleton && (weights[path] as f64) < cliff {
            pointer_files.push(path.clone());
            false
        } else {
            true
        }
    });
    if max_files > 0 && file_paths.len() > max_files {
        file_paths.truncate(max_files.max(protected_files.len()));
    }

    let mut lines = vec![
        format!("## Explore Results ({})", nodes.len()),
        String::new(),
    ];
    lines.extend(flow::format_flow_section(
        cg,
        &named_nodes,
        &query_tokens,
        &flow_plan,
    ));
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
    // Caller lists and location pointers must not consume the final envelope
    // before the evidence needed to explain the flow has been emitted.
    let mut footer = Vec::new();
    let blast_radius = flow::format_explore_blast_radius(cg, &nodes);
    if !blast_radius.is_empty() {
        footer.push(String::new());
        footer.extend(blast_radius);
    }
    footer.push(String::new());
    if !pointer_files.is_empty() {
        footer.push("### Additional symbol locations (source not included)".to_string());
        for path in pointer_files {
            for node in nodes.iter().filter(|node| node.file_path == path) {
                footer.push(format!("- {}:{} — {}", path, node.start_line, node.name));
            }
        }
        footer.push(String::new());
    }
    lines.push("### Source Code".to_string());

    let guidance = "\nUncovered source anchors are gaps; use another rustcodegraph_explore or rustcodegraph_node with exact names for missing evidence. Treat returned source as already Read.\n";
    // Exact byte costs include file headings, numbered lines and separators.
    let prefix = lines.join("\n");
    if prefix.len() + guidance.len() + 2 > budget.max_output_chars {
        // A flow description itself may exceed an explicitly tiny envelope.
        // Return gaps rather than a sliced path or falsely complete source.
        let mut result = String::new();
        for anchor in &flow_plan.anchors {
            let gap = format!("Uncovered anchor: {}:{}\n", anchor.file_path, anchor.line);
            if result.len() + gap.len() + guidance.len() <= budget.max_output_chars {
                result.push_str(&gap);
            }
        }
        if result.len() + guidance.len() <= budget.max_output_chars {
            result.push_str(guidance);
        }
        if result.is_empty() {
            for message in [
                "Source omitted; use rustcodegraph_explore or rustcodegraph_node for uncovered anchors.",
                "uncovered anchors",
                "gap",
            ] {
                if message.len() <= budget.max_output_chars {
                    return message.to_string();
                }
            }
        }
        return result;
    }
    let mut remaining = budget
        .max_output_chars
        .saturating_sub(prefix.len() + guidance.len() + 2);
    let minima: Vec<_> = file_paths
        .iter()
        .map(|path| {
            if protected_files.contains(path) {
                render::minimum_flow_section(
                    project_root,
                    path,
                    &cg.get_nodes_in_file(path),
                    &flow_plan.anchors,
                    budget.max_chars_per_file,
                )
            } else {
                String::new()
            }
        })
        .collect();
    let mut reserved: usize = minima.iter().map(|m| m.len() + 1).sum();
    let extras = super::budget::allocate_explore_source_budget(
        &file_paths.iter().map(|p| weights[p]).collect::<Vec<_>>(),
        remaining.saturating_sub(reserved) / 2,
    );
    let mut reclaimed = 0usize;
    let mut fallback = None;
    let mut has_source = false;
    for ((file_path, minimum), extra) in file_paths.into_iter().zip(minima).zip(extras) {
        reserved = reserved.saturating_sub(minimum.len() + 1);
        let allowance = minimum
            .len()
            .saturating_add(extra)
            .saturating_add(reclaimed);
        let cap = remaining
            .saturating_sub(reserved + 1)
            .min(budget.max_chars_per_file)
            .min(allowance);
        let mode = if protected_files.contains(&file_path) {
            render::ExploreFileMode::Full
        } else {
            render::explore_file_mode(
                &file_path,
                adaptive,
                &query_tokens,
                &unique_callable_files,
                &family,
            )
        };
        let mut original = render::render_explore_file_section(
            project_root,
            &file_path,
            &cg.get_nodes_in_file(&file_path),
            render::ExploreFileRenderOptions {
                mode,
                query_tokens: &source_tokens,
                anchors: &flow_plan.anchors,
                family: &family,
                max_chars_per_file: cap,
            },
        )
        .join("\n")
            + "\n";
        // Greedy whole-owner expansion may cost a later anchor. Keep the compact
        // reservation whenever it supplies evidence that expansion would omit.
        if !minimum.is_empty()
            && (original.len() > cap
                || original.contains("(no source available)")
                || (original.contains("uncovered anchor") && !minimum.contains("uncovered anchor")))
        {
            original = minimum;
        }
        if original.len() > cap {
            original = format!(
                "#### {file_path} · source omitted\n... (uncovered anchor lines: {}; source omitted)\n",
                flow_plan
                    .anchors
                    .iter()
                    .filter(|a| a.file_path == file_path)
                    .map(|a| a.line.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        let emitted = session
            .map(|s| s.deduplicate(project_root, &original))
            .unwrap_or_else(|| original.clone());
        if emitted.len() + 1 > remaining {
            let missing = flow_plan
                .anchors
                .iter()
                .filter(|a| a.file_path == file_path)
                .map(|a| a.line.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            let gap = format!(
                "\n#### {file_path} · source omitted\n... (uncovered anchor lines: {missing}; source omitted)\n"
            );
            if gap.len() <= remaining {
                remaining -= gap.len();
                lines.push(gap);
            }
            continue;
        }
        remaining -= emitted.len() + 1;
        reclaimed = allowance.saturating_sub(emitted.len());
        if emitted != original && fallback.is_none() {
            fallback = Some((lines.len(), original, emitted.len()));
        }
        has_source |= emitted.lines().any(|line| {
            line.split_once('\t')
                .is_some_and(|(n, _)| n.parse::<usize>().is_ok())
        });
        lines.push(emitted);
    }
    if !has_source
        && let Some((index, source, old_len)) = fallback
        && source.len() <= remaining + old_len
    {
        remaining = remaining + old_len - source.len();
        lines[index] = source;
    }
    lines.push(guidance.to_string());
    // Summaries use only the actual remainder; never truncate source to fit them.
    let summary_gap =
        "... (additional callers/locations omitted; use rustcodegraph_explore with exact names)";
    for line in footer {
        if line.len() + 1 + summary_gap.len() + 1 > remaining {
            if summary_gap.len() < remaining {
                lines.push(summary_gap.to_string());
            }
            break;
        }
        remaining -= line.len() + 1;
        lines.push(line);
    }

    lines.join("\n")
}

#[cfg(test)]
mod budget_tests {
    use super::*;
    #[test]
    fn tiny_envelopes_return_gaps_without_utf8_or_envelope_overflow() {
        let root = std::env::temp_dir().join(format!("rcg-tiny-budget-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("flow.ts"),
            "export function start() { end(); }\nexport function end() { return '中文🙂'; }\n",
        )
        .unwrap();
        let mut cg = crate::CodeGraph::init_sync(&root).unwrap();
        assert!(cg.index_all(crate::IndexOptions::default()).success);
        let nodes = cg
            .search_nodes("start", None)
            .into_iter()
            .chain(cg.search_nodes("end", None))
            .map(|r| r.node)
            .collect::<Vec<_>>();
        for tier in [1, 150, 500, 5000, 20000, 40000] {
            for cap in [0, 3, 100, 300, 700, 13000] {
                let mut budget = get_explore_output_budget(tier);
                budget.max_output_chars = cap;
                let text = format_explore_file_result(
                    &mut cg,
                    &root,
                    "start end",
                    nodes.clone(),
                    8,
                    &budget,
                    None,
                );
                assert!(
                    text.len() <= cap,
                    "tier={tier} cap={cap} bytes={}\n{text}",
                    text.len()
                );
                if (100..300).contains(&cap) {
                    assert!(text.contains("uncovered"));
                }
            }
        }
        cg.close();
        std::fs::remove_dir_all(root).unwrap();
    }
}
