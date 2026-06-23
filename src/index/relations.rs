//! 类关系、成员引用和 store action 的补充边。
//!
//! 这些边大多来自 fallback 对声明行或对象字面量的二次扫描，用于补齐原生抽取漏掉的继承、实现和成员调用。

use super::*;

pub(super) fn append_facade_relation_edges(
    source: &str,
    language: Language,
    nodes: &[Node],
    pending_edges: &mut Vec<RichFacadePendingEdge>,
) {
    // 关系补扫按源码行重新定位源节点，避免初次 fallback 和原生抽取混合后遗漏 extends/implements。
    let node_for = |name: &str, line_number: u64, kind: NodeKind| {
        nodes
            .iter()
            .find(|node| {
                node.name == name
                    && node.kind == kind
                    && node.start_line == line_number
                    && facade_supertype_bearing(node.kind)
            })
            .or_else(|| {
                nodes.iter().find(|node| {
                    node.name == name && node.kind == kind && facade_supertype_bearing(node.kind)
                })
            })
    };
    let mut seen = HashSet::new();

    for (idx, line) in source.lines().enumerate() {
        let line_number = (idx + 1) as u64;
        let trimmed = line.trim();
        let source_node = if let Some((class_name, container_kind)) = container_from_line(trimmed) {
            let node_kind = match container_kind {
                FacadeContainerKind::Class | FacadeContainerKind::Object => NodeKind::Class,
                FacadeContainerKind::Struct => NodeKind::Struct,
                FacadeContainerKind::Enum => NodeKind::Enum,
                FacadeContainerKind::Trait => NodeKind::Trait,
            };
            node_for(class_name, line_number, node_kind)
        } else if let Some(interface_name) = interface_name_from_line(trimmed) {
            node_for(interface_name, line_number, NodeKind::Interface)
        } else {
            None
        };
        let Some(source_node) = source_node else {
            continue;
        };

        for (kind, target_name) in class_relation_names_from_line(trimmed, "implements")
            .into_iter()
            .map(|target| (EdgeKind::Implements, target))
            .chain(
                class_relation_names_from_line(trimmed, "extends")
                    .into_iter()
                    .map(|target| (EdgeKind::Extends, target)),
            )
            .chain(
                (language == Language::Dart)
                    .then(|| class_relation_names_from_line(trimmed, "with"))
                    .into_iter()
                    .flatten()
                    .map(|target| (EdgeKind::Implements, target)),
            )
            .chain(
                (matches!(language, Language::C | Language::Cpp))
                    .then(|| cpp_base_class_names_from_line(trimmed))
                    .into_iter()
                    .flatten()
                    .map(|target| (EdgeKind::Extends, target)),
            )
        {
            let key = format!("{}|{:?}|{}", source_node.id, kind, target_name);
            if seen.insert(key) {
                pending_edges.push(RichFacadePendingEdge {
                    source: source_node.id.clone(),
                    target_name,
                    kind,
                    metadata: None,
                    line: Some(line_number),
                    column: Some(0),
                });
            }
        }

        if language == Language::Ruby
            && let Some(target_name) = ruby_superclass_from_line(trimmed)
        {
            let key = format!("{}|{:?}|{}", source_node.id, EdgeKind::Extends, target_name);
            if seen.insert(key) {
                pending_edges.push(RichFacadePendingEdge {
                    source: source_node.id.clone(),
                    target_name,
                    kind: EdgeKind::Extends,
                    metadata: None,
                    line: Some(line_number),
                    column: Some(0),
                });
            }
        }
    }
}

pub(super) fn append_facade_class_member_reference_edges(
    file_path: &str,
    source: &str,
    language: Language,
    indexed_at: TimestampMs,
    nodes: &[Node],
    pending_edges: &mut Vec<RichFacadePendingEdge>,
) {
    // 类成员已在节点阶段去重；这里只重放解析逻辑以收集成员签名中隐藏的调用/引用边。
    let source_lines = source.lines().collect::<Vec<_>>();
    let package_prefix = package_prefix_from_source(language, source);
    let mut in_class = false;
    let mut class_depth = 0isize;
    let mut current_class_name: Option<String> = None;
    let mut current_container_kind: Option<FacadeContainerKind> = None;

    for (idx, line) in source_lines.iter().copied().enumerate() {
        let line_number = (idx + 1) as u64;
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("//") {
            continue;
        }

        if let Some((class_name, container_kind)) = container_from_line(trimmed) {
            let delta = brace_delta(trimmed);
            if trimmed.contains('{') && delta > 0 {
                in_class = true;
                class_depth = delta;
                current_class_name = Some(class_name.to_owned());
                current_container_kind = Some(container_kind);
            }
            continue;
        }

        if !in_class {
            continue;
        }

        if let Some((member_node, mut refs)) = class_member_from_line(
            file_path,
            language,
            current_class_name.as_deref(),
            current_container_kind,
            line,
            line_number,
            indexed_at,
            package_prefix.as_deref(),
        ) {
            let Some(source_node) = nodes.iter().find(|node| {
                node.file_path == file_path
                    && node.name == member_node.name
                    && node.kind == member_node.kind
                    && node.start_line == member_node.start_line
            }) else {
                continue;
            };
            for reference in &mut refs {
                reference.source = source_node.id.clone();
            }
            pending_edges.append(&mut refs);
        }

        class_depth += brace_delta(trimmed);
        if class_depth <= 0 {
            in_class = false;
            class_depth = 0;
            current_class_name = None;
            current_container_kind = None;
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn append_facade_store_object_functions(
    file_path: &str,
    source: &str,
    language: Language,
    indexed_at: TimestampMs,
    file_node_id: &str,
    nodes: &mut Vec<Node>,
    pending_edges: &mut Vec<RichFacadePendingEdge>,
    direct_edges: &mut Vec<Edge>,
) {
    if !source.contains("export const") || !source.contains("create") {
        return;
    }
    // store action 会作为普通对象属性出现；提升为 Function 节点后 explore-flow 才能穿过状态管理层。
    for (object_start, object_end) in facade_store_object_ranges(source) {
        for (member_start, member_end) in
            facade_split_top_level_segments(source, object_start + 1, object_end)
        {
            let Some((name, value_start, value_end)) =
                facade_object_function_property(source, member_start, member_end)
            else {
                continue;
            };
            if nodes.iter().any(|node| {
                node.name == name
                    && node.kind == NodeKind::Function
                    && node.file_path == file_path
                    && node.start_line == facade_line_number_at(source, member_start)
            }) {
                continue;
            }
            let line_number = facade_line_number_at(source, member_start);
            let line = &source[member_start..member_end];
            let node = facade_node(
                file_path,
                language,
                &name,
                NodeKind::Function,
                line,
                line_number,
                Some(line.trim().to_owned()),
                None,
                false,
                indexed_at,
            );
            push_facade_executable_edges(
                pending_edges,
                &node.id,
                &source[value_start..value_end],
                Some(&name),
                line_number,
            );
            direct_edges.push(facade_contains_edge(file_node_id, &node.id, line_number));
            nodes.push(node);
        }
    }
}
