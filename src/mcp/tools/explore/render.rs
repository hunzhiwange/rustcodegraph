//! Source rendering helpers for `rustcodegraph_explore`.
//!
//! 渲染层负责在预算内给出可直接阅读/编辑定位的源码，同时对配置文件值做脱敏，
//! 并在大型继承族里用 skeleton/focused 模式减少无关实现体。

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

use super::super::budget::{explore_line_numbers_enabled, number_source_lines};
use super::super::response::char_boundary_before;
use super::super::shared::{config_keys, node_matches_symbol, project_file_path};
use crate::types::{EdgeKind, NodeKind};

pub(super) enum ExploreFileMode {
    Full,
    Skeleton,
    Focused,
}

pub(super) struct ExploreFamilyContext {
    sibling_files: HashSet<String>,
    base_names_by_file: HashMap<String, HashSet<String>>,
}

pub(super) fn explore_query_tokens(query: &str) -> HashSet<String> {
    query
        .split_whitespace()
        .filter_map(|token| {
            let token = token.trim_matches(|ch: char| {
                !ch.is_alphanumeric() && ch != '.' && ch != '_' && ch != '-'
            });
            (!token.is_empty()).then(|| token.to_string())
        })
        .collect()
}

pub(super) fn unique_named_callable_files(
    cg: &mut crate::CodeGraph,
    query_tokens: &HashSet<String>,
) -> HashSet<String> {
    let mut files = HashSet::new();
    for token in query_tokens {
        let matches = cg
            .search_nodes(token, None)
            .into_iter()
            .map(|result| result.node)
            .filter(|node| {
                matches!(node.kind, NodeKind::Function | NodeKind::Method)
                    && (node.name == *token
                        || node.qualified_name == *token
                        || node_matches_symbol(node, token))
            })
            .collect::<Vec<_>>();
        if matches.len() == 1 {
            files.insert(matches[0].file_path.clone());
        }
    }
    files
}

pub(super) fn explore_family_context(cg: &mut crate::CodeGraph) -> ExploreFamilyContext {
    // 当一个 interface/base class 有很多实现时，完整输出所有 sibling 文件
    // 会吞掉预算；先收集“大家族”关系，后面按模式渲染。
    let mut type_nodes = cg.get_nodes_by_kind(NodeKind::Interface);
    type_nodes.extend(cg.get_nodes_by_kind(NodeKind::Class));

    let mut sibling_files = HashSet::new();
    let mut base_names_by_file: HashMap<String, HashSet<String>> = HashMap::new();
    for supertype in type_nodes {
        let family_edges = cg
            .get_incoming_edges(&supertype.id)
            .into_iter()
            .filter(|edge| matches!(edge.kind, EdgeKind::Implements | EdgeKind::Extends))
            .collect::<Vec<_>>();
        if family_edges.len() < 3 {
            continue;
        }

        base_names_by_file
            .entry(supertype.file_path.clone())
            .or_default()
            .insert(supertype.name.clone());
        for edge in family_edges {
            if let Some(source) = cg.get_node(&edge.source) {
                sibling_files.insert(source.file_path);
            }
        }
    }

    ExploreFamilyContext {
        sibling_files,
        base_names_by_file,
    }
}

pub(super) fn explore_file_mode(
    file_path: &str,
    adaptive: bool,
    query_tokens: &HashSet<String>,
    unique_callable_files: &HashSet<String>,
    family: &ExploreFamilyContext,
) -> ExploreFileMode {
    // 用户明确命中的唯一 callable 文件保持 full；大家族 sibling 默认 skeleton，
    // base 文件若被查询命中则 focused 展开相关类型。
    if !adaptive {
        return ExploreFileMode::Full;
    }

    if family
        .base_names_by_file
        .get(file_path)
        .map(|names| names.iter().any(|name| query_tokens.contains(name)))
        .unwrap_or(false)
    {
        return ExploreFileMode::Focused;
    }

    if family.sibling_files.contains(file_path) && !unique_callable_files.contains(file_path) {
        return ExploreFileMode::Skeleton;
    }

    ExploreFileMode::Full
}

pub(super) fn render_explore_file_section(
    project_root: &Path,
    file_path: &str,
    mode: ExploreFileMode,
    query_tokens: &HashSet<String>,
    family: &ExploreFamilyContext,
    max_chars_per_file: usize,
) -> Vec<String> {
    let source = fs::read_to_string(project_file_path(project_root, file_path)).unwrap_or_default();
    let label = match mode {
        ExploreFileMode::Full => "full",
        ExploreFileMode::Skeleton => "skeleton (signatures only)",
        ExploreFileMode::Focused => "focused",
    };
    let label = if is_config_file_path(file_path) {
        "config values withheld"
    } else {
        label
    };
    let mut lines = vec![format!("#### {file_path} · {label}")];
    let rendered = if is_config_file_path(file_path) {
        render_config_explore_summary(&source)
    } else {
        match mode {
            ExploreFileMode::Full => source,
            ExploreFileMode::Skeleton => render_signature_skeleton(&source),
            ExploreFileMode::Focused => {
                let focused_names = family
                    .base_names_by_file
                    .get(file_path)
                    .map(|names| {
                        names
                            .iter()
                            .filter(|name| query_tokens.contains(*name))
                            .cloned()
                            .collect::<HashSet<_>>()
                    })
                    .unwrap_or_default();
                render_focused_family_source(&source, &focused_names)
            }
        }
    };
    let rendered = truncate_explore_file_body(&rendered, max_chars_per_file);
    let rendered = if explore_line_numbers_enabled() && !rendered.trim().is_empty() {
        number_source_lines(&rendered, 1)
    } else {
        rendered
    };
    if rendered.trim().is_empty() {
        lines.push("(no source available)".to_string());
    } else {
        lines.extend(rendered.lines().map(str::to_string));
    }
    lines
}

fn truncate_explore_file_body(source: &str, max_chars: usize) -> String {
    if max_chars == 0 || source.len() <= max_chars {
        return source.to_string();
    }
    let safe_len = char_boundary_before(source, max_chars.min(source.len()));
    let head = &source[..safe_len];
    let cut = head
        .rfind('\n')
        .filter(|idx| *idx > safe_len * 8 / 10)
        .unwrap_or(safe_len);
    format!(
        "{}\n\n... (section trimmed; use rustcodegraph_explore or rustcodegraph_node with exact names for more source; do NOT Read)",
        &head[..cut]
    )
}

fn is_config_file_path(file_path: &str) -> bool {
    let lower = file_path.to_ascii_lowercase();
    lower.ends_with(".properties") || lower.ends_with(".yml") || lower.ends_with(".yaml")
}

fn render_config_explore_summary(source: &str) -> String {
    // 配置文件可能包含密钥或连接串；只暴露键名，让 agent 知道结构但看不到值。
    let keys = config_keys(source);
    let mut lines = vec!["Config/data file values withheld.".to_string()];
    if !keys.is_empty() {
        lines.push("Keys:".to_string());
        lines.extend(keys.into_iter().map(|key| format!("- `{key}`")));
    }
    lines.join("\n")
}

fn render_signature_skeleton(source: &str) -> String {
    let mut lines = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("import ") {
            continue;
        }
        if is_type_declaration_line(trimmed) {
            lines.push(skeleton_type_declaration(trimmed));
            continue;
        }
        if let Some(signature) = skeleton_callable_signature(trimmed) {
            lines.push(format!("  {signature}"));
        }
    }
    lines.join("\n")
}

fn render_focused_family_source(source: &str, focused_names: &HashSet<String>) -> String {
    let mut lines = Vec::new();
    let mut in_class = false;
    let mut include_full = false;
    let mut depth = 0isize;

    for line in source.lines() {
        let trimmed = line.trim();
        if !in_class {
            if let Some(class_name) = explore_class_name_from_line(trimmed) {
                include_full = focused_names.contains(class_name);
                in_class = true;
                depth = brace_delta_for_text(trimmed);
                if include_full {
                    lines.push(line.to_string());
                } else {
                    lines.push(skeleton_type_declaration(trimmed));
                }
                if depth <= 0 {
                    in_class = false;
                    include_full = false;
                }
                continue;
            }
            if !trimmed.is_empty() && !trimmed.starts_with("import ") {
                lines.push(line.to_string());
            }
            continue;
        }

        if include_full {
            lines.push(line.to_string());
        } else if let Some(signature) = skeleton_callable_signature(trimmed) {
            lines.push(format!("  {signature}"));
        }

        depth += brace_delta_for_text(trimmed);
        if depth <= 0 {
            in_class = false;
            include_full = false;
        }
    }

    lines.join("\n")
}

fn is_type_declaration_line(line: &str) -> bool {
    explore_class_name_from_line(line).is_some() || explore_interface_name_from_line(line).is_some()
}

fn skeleton_type_declaration(line: &str) -> String {
    if let Some(open) = line.find('{') {
        format!("{} {{ ... }}", line[..open].trim_end())
    } else {
        line.to_string()
    }
}

fn skeleton_callable_signature(line: &str) -> Option<String> {
    if line.starts_with("return ") || line.starts_with("const ") || line.starts_with("let ") {
        return None;
    }
    let paren = line.find('(')?;
    let before = line[..paren].trim_end();
    if before.is_empty() || before.contains('=') {
        return None;
    }
    let open = line.find('{').unwrap_or(line.len());
    let signature = line[..open].trim().trim_end_matches(';').trim();
    (!signature.is_empty()).then(|| format!("{signature};"))
}

fn explore_class_name_from_line(line: &str) -> Option<&str> {
    explore_type_name_from_line(line, "class")
}

fn explore_interface_name_from_line(line: &str) -> Option<&str> {
    explore_type_name_from_line(line, "interface")
}

fn explore_type_name_from_line<'a>(line: &'a str, keyword: &str) -> Option<&'a str> {
    let mut parts = line.split_whitespace().peekable();
    while matches!(
        parts.peek(),
        Some(&"export" | &"default" | &"abstract" | &"declare")
    ) {
        parts.next();
    }
    if parts.next()? != keyword {
        return None;
    }
    parts
        .next()
        .map(|name| name.trim_matches(|ch: char| !is_explore_identifier_char(ch)))
}

fn is_explore_identifier_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_' || ch == '$'
}

fn brace_delta_for_text(input: &str) -> isize {
    input.chars().filter(|ch| *ch == '{').count() as isize
        - input.chars().filter(|ch| *ch == '}').count() as isize
}
