//! Extra dynamic-dispatch notes for `rustcodegraph_explore`.
//!
//! explore 的主路径来自图边；这里补充“图里没有静态边但源码显示存在运行时分派”
//! 的提示，帮助 agent 停在正确边界，而不是凭空推断调用链。

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

use regex::Regex;

use crate::mcp::dynamic_boundaries::{BoundaryMatch, scan_dynamic_dispatch};
use crate::types::{Node, NodeKind};

use super::super::shared::{dedupe_nodes, language_label, project_file_path};

pub(super) fn format_dynamic_dispatch_links(
    cg: &mut crate::CodeGraph,
    project_root: &Path,
    named_nodes: &[Node],
) -> Vec<String> {
    // 只在用户命名的节点之间合成展示用 event link，避免全仓扫描 event bus
    // 产生高噪声路径。
    if named_nodes.len() < 2 {
        return Vec::new();
    }
    let files = cg.get_files();
    let mut source_by_file = HashMap::new();
    for file in files {
        let path = project_file_path(project_root, &file.path);
        let source = fs::read_to_string(path).unwrap_or_default();
        source_by_file.insert(file.path, source);
    }

    let mut rows = Vec::new();
    let mut seen = HashSet::new();
    for source_node in named_nodes {
        let Some(source) = source_by_file.get(&source_node.file_path) else {
            continue;
        };
        for (event, emit_line) in literal_event_emits(source) {
            for target_node in named_nodes {
                if target_node.id == source_node.id {
                    continue;
                }
                let Some((registered_file, registered_line)) =
                    find_event_registration(&source_by_file, &event, &target_node.name)
                else {
                    continue;
                };
                let key = format!("{}:{}:{event}", source_node.id, target_node.id);
                if !seen.insert(key) {
                    continue;
                }
                let wiring = if registered_file.is_empty() {
                    format!("@{}:{emit_line}", source_node.file_path)
                } else {
                    format!("@{registered_file}:{registered_line}")
                };
                rows.push(format!(
                    "- {} → {}   [dynamic: event {event} {wiring}]",
                    source_node.name, target_node.name
                ));
            }
        }
    }

    if rows.is_empty() {
        Vec::new()
    } else {
        let mut lines = vec![
            "## Dynamic-dispatch links among your symbols".to_string(),
            "(synthesized — indirect runtime hops surfaced from registry/bus wiring)".to_string(),
            String::new(),
        ];
        lines.extend(rows);
        lines
    }
}

fn literal_event_emits(source: &str) -> Vec<(String, u64)> {
    let Ok(re) = Regex::new(r#"\.emit\s*\(\s*['"]([^'"]+)['"]"#) else {
        return Vec::new();
    };
    re.captures_iter(source)
        .filter_map(|cap| {
            let event = cap.get(1)?.as_str().to_string();
            let start = cap.get(0)?.start();
            Some((event, line_number_at(source, start)))
        })
        .collect()
}

fn find_event_registration(
    source_by_file: &HashMap<String, String>,
    event: &str,
    handler_name: &str,
) -> Option<(String, u64)> {
    let pattern = format!(
        r#"\.on\s*\(\s*['"]{}['"]\s*,\s*{}(?:\s|,|\))"#,
        regex::escape(event),
        regex::escape(handler_name)
    );
    let re = Regex::new(&pattern).ok()?;
    for (file_path, source) in source_by_file {
        let Some(mat) = re.find(source) else {
            continue;
        };
        return Some((file_path.clone(), line_number_at(source, mat.start())));
    }
    None
}

pub(super) fn format_dynamic_boundaries(
    cg: &mut crate::CodeGraph,
    project_root: &Path,
    named_nodes: &[Node],
    query_tokens: &HashSet<String>,
) -> Vec<String> {
    if named_nodes.is_empty() {
        return Vec::new();
    }
    let named_names = named_nodes
        .iter()
        .map(|node| node.name.clone())
        .collect::<HashSet<_>>();
    let mut notes = Vec::new();
    let mut seen_sites = HashSet::new();
    for node in named_nodes.iter().take(8) {
        if notes.len() >= 4 {
            break;
        }
        let source = fs::read_to_string(project_file_path(project_root, &node.file_path))
            .unwrap_or_default();
        if source.is_empty() {
            continue;
        }
        let language = language_label(node.language);
        for boundary in scan_dynamic_dispatch(&source, &language, 1) {
            if notes.len() >= 4 {
                break;
            }
            let site_key = format!("{}:{}:{}", node.file_path, boundary.line, boundary.form);
            if !seen_sites.insert(site_key) {
                continue;
            }
            push_dynamic_boundary_note(cg, &mut notes, node, &boundary, &named_names, query_tokens);
        }
    }

    if notes.is_empty() {
        Vec::new()
    } else {
        let mut lines = vec![
            "## Dynamic boundaries (the static path ends at runtime dispatch)".to_string(),
            String::new(),
        ];
        lines.extend(notes);
        lines.push(String::new());
        lines.push(
            "> These sites choose their target at runtime; the source for the sites above is included below."
                .to_string(),
        );
        lines
    }
}

fn push_dynamic_boundary_note(
    cg: &mut crate::CodeGraph,
    notes: &mut Vec<String>,
    node: &Node,
    boundary: &BoundaryMatch,
    named_names: &HashSet<String>,
    query_tokens: &HashSet<String>,
) {
    let more = boundary
        .more_sites
        .map(|count| {
            format!(
                " (+{count} more such site{})",
                if count > 1 { "s" } else { "" }
            )
        })
        .unwrap_or_default();
    notes.push(format!(
        "- `{}` ({}:{}) — {}: `{}`{}",
        node.name, node.file_path, boundary.line, boundary.label, boundary.snippet, more
    ));
    if let Some(key) = boundary.key.as_deref()
        && let Some(candidates) = dynamic_boundary_candidates(
            cg,
            key,
            boundary.key_is_type.unwrap_or(false),
            named_names,
            query_tokens,
            &node.id,
        )
    {
        notes.push(format!("  {candidates}"));
    }
}

fn dynamic_boundary_candidates(
    cg: &mut crate::CodeGraph,
    key: &str,
    key_is_type: bool,
    named_names: &HashSet<String>,
    query_tokens: &HashSet<String>,
    self_id: &str,
) -> Option<String> {
    // key 是运行时字符串/类型名时，候选排序优先用户查询里已经命名的符号；
    // 这些候选只是提示，不写回图。
    let key_norm = normalize_candidate_key(key);
    if key_norm.len() < 3 {
        return None;
    }
    let mut candidates = Vec::new();
    let cap = capitalize_first(key);
    let probes = if key_is_type {
        vec![format!("{key}Handler"), key.to_string()]
    } else {
        vec![
            key.to_string(),
            format!("on{cap}"),
            format!("handle{cap}"),
            format!("{key}Handler"),
            format!("handle_{key}"),
        ]
    };
    for probe in probes {
        candidates.extend(cg.get_nodes_by_name(&probe));
    }
    candidates.extend(
        cg.search_nodes(key, None)
            .into_iter()
            .map(|result| result.node),
    );
    candidates.retain(|node| {
        node.id != self_id
            && is_dynamic_candidate_node(node)
            && candidate_matches_key(&node.name, &key_norm)
    });
    dedupe_nodes(&mut candidates);
    candidates.sort_by(|a, b| {
        let a_named = named_names.contains(&a.name) || query_tokens.contains(&a.name);
        let b_named = named_names.contains(&b.name) || query_tokens.contains(&b.name);
        b_named
            .cmp(&a_named)
            .then_with(|| a.file_path.cmp(&b.file_path))
            .then_with(|| a.start_line.cmp(&b.start_line))
    });

    if candidates.is_empty() {
        return None;
    }
    let rendered = candidates
        .into_iter()
        .take(4)
        .map(|node| {
            let named = named_names.contains(&node.name) || query_tokens.contains(&node.name);
            format!(
                "`{}` ({}:{}){}",
                node.name,
                node.file_path,
                node.start_line,
                if named { " ← you named this" } else { "" }
            )
        })
        .collect::<Vec<_>>();
    Some(format!(
        "candidates for key `{key}`: {}",
        rendered.join(", ")
    ))
}

fn is_dynamic_candidate_node(node: &Node) -> bool {
    matches!(
        node.kind,
        NodeKind::Function | NodeKind::Method | NodeKind::Class | NodeKind::Component
    )
}

fn candidate_matches_key(name: &str, key_norm: &str) -> bool {
    let name_norm = normalize_candidate_key(name);
    name_norm.len() >= 3 && (name_norm.contains(key_norm) || key_norm.contains(&name_norm))
}

fn normalize_candidate_key(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn capitalize_first(value: &str) -> String {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    first.to_uppercase().chain(chars).collect()
}

fn line_number_at(source: &str, byte_index: usize) -> u64 {
    source
        .as_bytes()
        .iter()
        .take(byte_index.min(source.len()))
        .filter(|byte| **byte == b'\n')
        .count() as u64
        + 1
}
