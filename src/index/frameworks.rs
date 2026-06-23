//! facade 索引阶段的框架级补充抽取。
//!
//! framework resolver 会产出 route/component 等节点以及“按名称待解析”的引用；这里把它们接入同一套
//! pending-edge 管线，避免为框架节点维护第二套解析逻辑。

use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn append_facade_framework_extraction(
    file_path: &str,
    source: &str,
    language: Language,
    framework_resolvers: &[ResolverRef],
    file_node_id: &str,
    nodes: &mut Vec<Node>,
    pending_edges: &mut Vec<RichFacadePendingEdge>,
    direct_edges: &mut Vec<Edge>,
) {
    let mut seen_nodes = nodes
        .iter()
        .map(|node| node.id.clone())
        .collect::<HashSet<_>>();

    for resolver in framework_resolvers {
        if resolver
            .languages()
            .is_some_and(|languages| !languages.contains(&language))
        {
            continue;
        }
        let mut result = resolver.extract(file_path, source);
        for node in result.nodes.drain(..) {
            if seen_nodes.insert(node.id.clone()) {
                // 框架节点也挂在 file 节点下，保证文件视图和 context 构建能稳定找到它们。
                direct_edges.push(facade_contains_edge(
                    file_node_id,
                    &node.id,
                    node.start_line,
                ));
                nodes.push(node);
            }
        }
        pending_edges.extend(
            result
                .references
                .drain(..)
                .map(|reference| RichFacadePendingEdge {
                    source: reference.from_node_id,
                    target_name: reference.reference_name,
                    kind: edge_kind_from_reference_kind(reference.reference_kind),
                    metadata: None,
                    line: Some(reference.line),
                    column: Some(reference.column),
                }),
        );
    }
}

pub(super) fn append_facade_csharp_inline_namespace_types(
    file_path: &str,
    source: &str,
    indexed_at: TimestampMs,
    file_node_id: &str,
    nodes: &mut Vec<Node>,
    direct_edges: &mut Vec<Edge>,
) {
    // file-scoped/inline namespace 写法会把 namespace 和类型放在同一行；部分解析器版本会漏掉这种类型。
    for (idx, line) in source.lines().enumerate() {
        let line_number = (idx + 1) as u64;
        let trimmed = line.trim();
        let Some(after_namespace) = trimmed.strip_prefix("namespace ") else {
            continue;
        };
        let Some((namespace, after_brace)) = after_namespace.split_once('{') else {
            continue;
        };
        let namespace = trim_identifier(namespace.trim());
        if namespace.is_empty() {
            continue;
        }
        let mut rest = after_brace.trim_start();
        loop {
            let mut changed = false;
            for prefix in [
                "public ",
                "private ",
                "protected ",
                "internal ",
                "abstract ",
                "sealed ",
                "static ",
                "partial ",
            ] {
                if let Some(after) = rest.strip_prefix(prefix) {
                    rest = after.trim_start();
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        let loose_rest = rest;
        let (kind, after_keyword) = if let Some(after) = loose_rest
            .strip_prefix("class ")
            .or_else(|| csharp_after_keyword(loose_rest, "class"))
        {
            (NodeKind::Class, after)
        } else if let Some(after) = loose_rest
            .strip_prefix("record ")
            .or_else(|| csharp_after_keyword(loose_rest, "record"))
        {
            (NodeKind::Class, after)
        } else if let Some(after) = loose_rest
            .strip_prefix("struct ")
            .or_else(|| csharp_after_keyword(loose_rest, "struct"))
        {
            (NodeKind::Struct, after)
        } else if let Some(after) = loose_rest
            .strip_prefix("interface ")
            .or_else(|| csharp_after_keyword(loose_rest, "interface"))
        {
            (NodeKind::Interface, after)
        } else {
            continue;
        };
        let Some(name) = first_identifier(after_keyword).map(trim_identifier) else {
            continue;
        };
        if name.is_empty() {
            continue;
        }
        let mut node = facade_node(
            file_path,
            Language::CSharp,
            name,
            kind,
            line,
            line_number,
            Some(trimmed.to_owned()),
            None,
            rest.contains(" static "),
            indexed_at,
        );
        node.qualified_name = format!("{namespace}::{name}");
        if nodes.iter().any(|existing| {
            existing.file_path == file_path
                && existing.kind == kind
                && existing.name == name
                && existing.qualified_name == node.qualified_name
        }) {
            continue;
        }
        let node_id = node.id.clone();
        nodes.push(node);
        direct_edges.push(facade_contains_edge(file_node_id, &node_id, line_number));
    }
}

pub(super) fn csharp_after_keyword<'a>(input: &'a str, keyword: &str) -> Option<&'a str> {
    let pattern = format!(" {keyword} ");
    let start = input.find(&pattern)?;
    Some(input[start + pattern.len()..].trim_start())
}
