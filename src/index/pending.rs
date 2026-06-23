//! facade 抽取阶段的待解析边和去重策略。
//!
//! fallback 会先用目标名称记录关系，待所有节点收集完成后再解析成真实 Edge；这里也集中处理原生抽取和
//! fallback 结果合并时的重复节点、重复边和语言补洞开关。

use super::*;

#[derive(Debug, Clone)]
pub(super) struct RichFacadePendingEdge {
    // target_name 是未解析的符号名；只有 resolve_facade_edges_rich 成功后才会变成 target id。
    pub(super) source: String,
    pub(super) target_name: String,
    pub(super) kind: EdgeKind,
    pub(super) metadata: Option<HashMap<String, Value>>,
    pub(super) line: Option<u64>,
    pub(super) column: Option<u64>,
}

pub(super) fn facade_unresolved_reference(
    edge: &RichFacadePendingEdge,
    file_path: &str,
    language: Language,
) -> UnresolvedReference {
    UnresolvedReference {
        from_node_id: edge.source.clone(),
        reference_name: edge.target_name.clone(),
        reference_kind: facade_reference_kind(edge),
        line: edge.line.unwrap_or(0),
        column: edge.column.unwrap_or(0),
        file_path: Some(file_path.to_owned()),
        language: Some(language),
        candidates: None,
    }
}

pub(super) fn facade_reference_kind(edge: &RichFacadePendingEdge) -> ReferenceKind {
    if edge.kind == EdgeKind::References
        && edge
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("fnRef"))
            .and_then(Value::as_bool)
            == Some(true)
    {
        ReferenceKind::FunctionRef
    } else {
        ReferenceKind::from(edge.kind)
    }
}

pub(super) fn edge_kind_from_reference_kind(kind: ReferenceKind) -> EdgeKind {
    match kind {
        ReferenceKind::Contains => EdgeKind::Contains,
        ReferenceKind::Calls => EdgeKind::Calls,
        ReferenceKind::Imports => EdgeKind::Imports,
        ReferenceKind::Exports => EdgeKind::Exports,
        ReferenceKind::Extends => EdgeKind::Extends,
        ReferenceKind::Implements => EdgeKind::Implements,
        ReferenceKind::References | ReferenceKind::FunctionRef => EdgeKind::References,
        ReferenceKind::TypeOf => EdgeKind::TypeOf,
        ReferenceKind::Returns => EdgeKind::Returns,
        ReferenceKind::Instantiates => EdgeKind::Instantiates,
        ReferenceKind::Overrides => EdgeKind::Overrides,
        ReferenceKind::Decorates => EdgeKind::Decorates,
    }
}

pub(super) fn dedupe_facade_nodes(nodes: &mut Vec<Node>) {
    let mut seen = HashSet::new();
    nodes.retain(|node| seen.insert(node.id.clone()));
}

pub(super) fn dedupe_facade_edges(edges: &mut Vec<Edge>) {
    let mut seen = HashSet::new();
    edges.retain(|edge| {
        // calls 边忽略 metadata 去重，因为不同抽取路径可能给同一调用附带不同但不影响遍历的元数据。
        let metadata = if edge.kind == EdgeKind::Calls {
            String::new()
        } else {
            edge.metadata
                .as_ref()
                .and_then(|metadata| serde_json::to_string(metadata).ok())
                .unwrap_or_default()
        };
        seen.insert(format!(
            "{}|{}|{:?}|{:?}|{:?}|{:?}|{}",
            edge.source, edge.target, edge.kind, edge.line, edge.column, edge.provenance, metadata
        ))
    });
}

pub(super) fn dedupe_facade_edges_by_node_names(edges: &mut Vec<Edge>, nodes: &[Node]) {
    // 原生和 fallback 可能生成不同 id 但指向同一源码符号；按节点名称语义再做一轮去重。
    let by_id = nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect::<HashMap<_, _>>();
    let mut seen = HashMap::new();
    let mut deduped = Vec::new();
    for edge in std::mem::take(edges) {
        let key = facade_edge_name_dedupe_key(&edge, &by_id);
        if let Some(index) = seen.get(&key).copied() {
            if facade_prefer_named_duplicate_edge(&edge, &deduped[index], &by_id) {
                deduped[index] = edge;
            }
        } else {
            seen.insert(key, deduped.len());
            deduped.push(edge);
        }
    }
    *edges = deduped;
}

pub(super) fn facade_edge_name_dedupe_key(edge: &Edge, by_id: &HashMap<&str, &Node>) -> String {
    let source = by_id.get(edge.source.as_str());
    let target = by_id.get(edge.target.as_str());
    let is_fn_ref = facade_edge_is_fn_ref(edge);
    let source_name = source
        .map(|node| facade_edge_node_dedupe_key(node, edge.kind, is_fn_ref))
        .unwrap_or_else(|| edge.source.clone());
    let target_name = target
        .map(|node| facade_edge_node_dedupe_key(node, edge.kind, is_fn_ref))
        .unwrap_or_else(|| edge.target.clone());
    let metadata = if edge.kind == EdgeKind::Calls {
        String::new()
    } else {
        edge.metadata
            .as_ref()
            .and_then(|metadata| serde_json::to_string(metadata).ok())
            .unwrap_or_default()
    };
    format!("{source_name}|{target_name}|{:?}|{metadata}", edge.kind)
}

pub(super) fn facade_prefer_named_duplicate_edge(
    candidate: &Edge,
    current: &Edge,
    by_id: &HashMap<&str, &Node>,
) -> bool {
    facade_named_duplicate_edge_score(candidate, by_id)
        > facade_named_duplicate_edge_score(current, by_id)
}

pub(super) fn facade_named_duplicate_edge_score(
    edge: &Edge,
    by_id: &HashMap<&str, &Node>,
) -> (u8, u8, u8) {
    let source = by_id.get(edge.source.as_str()).copied();
    let target = by_id.get(edge.target.as_str()).copied();
    (
        (edge.kind == EdgeKind::Calls && target.is_some_and(facade_cpp_canonical_node)) as u8,
        target
            .and_then(|node| node.signature.as_deref())
            .is_some_and(|signature| signature.contains('{')) as u8,
        source
            .and_then(|node| node.signature.as_deref())
            .is_some_and(|signature| signature.contains('{')) as u8,
    )
}

pub(super) fn facade_cpp_canonical_node(node: &Node) -> bool {
    matches!(node.language, Language::C | Language::Cpp) && node.qualified_name.contains("::")
}

pub(super) fn facade_edge_node_dedupe_key(
    node: &Node,
    edge_kind: EdgeKind,
    is_fn_ref: bool,
) -> String {
    if edge_kind == EdgeKind::Calls || is_fn_ref {
        format!(
            "{}:{}:{}:{:?}",
            node.file_path, node.start_line, node.name, node.kind
        )
    } else {
        node.qualified_name.clone()
    }
}

pub(super) fn facade_edge_is_fn_ref(edge: &Edge) -> bool {
    edge.kind == EdgeKind::References
        && edge
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("fnRef"))
            .and_then(Value::as_bool)
            == Some(true)
}

pub(super) const FACADE_NATIVE_MAX_BYTES: usize = 128 * 1024;
pub(super) const FACADE_NATIVE_MAX_LINES: usize = 5_000;

pub(super) fn should_index_with_native_parser(language: Language, source: &str) -> bool {
    // 大文件走文本 fallback，避免 tree-sitter 解析超大生成文件拖慢索引或占用过多内存。
    if source.len() > FACADE_NATIVE_MAX_BYTES || source.lines().count() > FACADE_NATIVE_MAX_LINES {
        return false;
    }
    matches!(
        language,
        Language::TypeScript
            | Language::Tsx
            | Language::JavaScript
            | Language::Jsx
            | Language::Python
            | Language::Go
            | Language::Rust
            | Language::Java
            | Language::C
            | Language::Cpp
            | Language::CSharp
            | Language::Php
            | Language::Ruby
            | Language::Swift
            | Language::Kotlin
            | Language::Scala
            | Language::Dart
            | Language::ObjC
            | Language::Lua
            | Language::Luau
            | Language::Svelte
            | Language::Vue
            | Language::Astro
            | Language::Razor
            | Language::Liquid
            | Language::Xml
    )
}

pub(super) fn go_facade_fallback_file(file_path: &str, source: &str) -> bool {
    file_path.ends_with(".pb.go")
        || file_path.ends_with("_grpc.pb.go")
        || source.contains("type msgServer")
}

pub(super) fn needs_facade_value_ref_fallback(language: Language, source: &str) -> bool {
    matches!(
        language,
        Language::C
            | Language::Cpp
            | Language::CSharp
            | Language::Ruby
            | Language::Scala
            | Language::Kotlin
            | Language::Swift
    ) && source
        .lines()
        .flat_map(facade_declared_value_names)
        .any(|name| distinctive_value_name(&name))
}

pub(super) fn pending_edge_from_unresolved_reference(
    reference: &UnresolvedReference,
) -> Option<RichFacadePendingEdge> {
    if reference.reference_name.is_empty() {
        return None;
    }
    let metadata = (reference.reference_kind == ReferenceKind::FunctionRef)
        .then(|| HashMap::from([("fnRef".to_owned(), json!(true))]));
    Some(RichFacadePendingEdge {
        source: reference.from_node_id.clone(),
        target_name: reference.reference_name.clone(),
        kind: edge_kind_from_reference_kind(reference.reference_kind),
        metadata,
        line: Some(reference.line),
        column: Some(reference.column),
    })
}

pub(super) fn package_prefix_from_source(language: Language, source: &str) -> Option<String> {
    match language {
        Language::Java | Language::Kotlin | Language::Scala => source.lines().find_map(|line| {
            let trimmed = line.trim();
            let rest = trimmed.strip_prefix("package ")?;
            Some(trim_identifier(rest.trim_end_matches(';')).to_owned())
                .filter(|name| !name.is_empty())
        }),
        Language::Php => source.lines().find_map(|line| {
            let trimmed = line.trim();
            let rest = trimmed.strip_prefix("namespace ")?;
            Some(trim_identifier(rest.trim_end_matches(';')).to_owned())
                .filter(|name| !name.is_empty())
        }),
        Language::CSharp => source.lines().find_map(|line| {
            let trimmed = line.trim();
            let rest = trimmed.strip_prefix("namespace ")?;
            let name = rest.split(['{', ';']).next().unwrap_or(rest).trim();
            Some(trim_identifier(name).to_owned()).filter(|name| !name.is_empty())
        }),
        _ => None,
    }
}

pub(super) fn apply_native_source_namespace(
    file_path: &str,
    source: &str,
    language: Language,
    nodes: &mut [Node],
) {
    let Some(prefix) = package_prefix_from_source(language, source) else {
        return;
    };
    // 原生抽取常以文件路径作 qualified_name 前缀；这里替换成语言 namespace/package，提升跨文件匹配率。
    for node in nodes.iter_mut().filter(|node| {
        node.file_path == file_path
            && !matches!(
                node.kind,
                NodeKind::File | NodeKind::Import | NodeKind::Namespace
            )
    }) {
        let file_prefix = format!("{file_path}::");
        if let Some(tail) = node.qualified_name.strip_prefix(&file_prefix) {
            let prefix_marker = format!("{prefix}::");
            node.qualified_name = if tail.starts_with(&prefix_marker) {
                tail.to_owned()
            } else {
                format!("{prefix}::{tail}")
            };
        } else if node.qualified_name == node.name {
            node.qualified_name = format!("{prefix}::{}", node.name);
        }
    }
}

pub(super) fn apply_native_ts_js_class_field_kinds(
    file_path: &str,
    source: &str,
    language: Language,
    nodes: &mut [Node],
) {
    // TS/JS class field 可能被通用抽取识别成 method；用 facade 成员解析结果回填 kind/signature。
    if !matches!(
        language,
        Language::TypeScript | Language::Tsx | Language::JavaScript | Language::Jsx
    ) {
        return;
    }

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

        if let Some((member_node, _)) = class_member_from_line(
            file_path,
            language,
            current_class_name.as_deref(),
            current_container_kind,
            line,
            line_number,
            0,
            package_prefix.as_deref(),
        ) && member_node.kind == NodeKind::Property
            && let Some(node) = nodes.iter_mut().find(|node| {
                node.file_path == file_path
                    && node.name == member_node.name
                    && node.start_line == member_node.start_line
                    && node.kind == NodeKind::Method
            })
        {
            node.kind = NodeKind::Property;
            node.signature = member_node.signature;
            node.visibility = member_node.visibility;
            node.is_static = member_node.is_static;
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
