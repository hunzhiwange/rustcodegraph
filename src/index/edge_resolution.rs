use super::*;

// facade 解析先收集 RichFacadePendingEdge，再在全文件节点集合上统一挑 target。
// 这样可以利用同文件/同目录/同语言家族等上下文评分，而不是边抽取时立即猜测。
pub(super) fn resolve_facade_edges_rich(
    nodes: &[Node],
    pending_edges: Vec<RichFacadePendingEdge>,
    existing_edges: &[Edge],
) -> Vec<Edge> {
    let mut by_name: HashMap<&str, Vec<&Node>> = HashMap::new();
    let mut by_id: HashMap<&str, &Node> = HashMap::new();
    for node in nodes {
        by_name.entry(&node.name).or_default().push(node);
        by_id.insert(&node.id, node);
    }
    // 函数引用解析会用到类型层级，先合并已有边和本轮 pending 里的继承信息。
    let supertypes = facade_supertype_edges(nodes, &pending_edges, existing_edges, &by_name);

    let mut seen = HashSet::new();
    let mut edges = Vec::new();
    for pending in pending_edges {
        if facade_pending_is_fn_ref(&pending) {
            let Some(edge) = resolve_facade_fn_ref(&pending, &by_name, &by_id, &supertypes) else {
                continue;
            };
            let key = format!("{}:{}:{:?}", edge.source, edge.target, edge.kind);
            if seen.insert(key) {
                edges.push(edge);
            }
            continue;
        }

        let Some(source) = by_id.get(pending.source.as_str()).copied() else {
            continue;
        };
        let Some(target) = resolve_facade_pending_target(&pending, source, nodes, &by_name) else {
            continue;
        };
        let key = format!("{}:{}:{:?}", pending.source, target.id, pending.kind);
        if !seen.insert(key) {
            continue;
        }
        edges.push(Edge {
            source: pending.source,
            target: target.id.clone(),
            kind: pending.kind,
            metadata: pending.metadata,
            line: pending.line,
            column: pending.column,
            provenance: None,
        });
    }
    edges
}

pub(super) fn resolve_facade_pending_target<'a>(
    pending: &RichFacadePendingEdge,
    source: &Node,
    nodes: &'a [Node],
    by_name: &HashMap<&str, Vec<&'a Node>>,
) -> Option<&'a Node> {
    // 解析优先级：完整 qualified name -> 同名候选评分 -> 调用/配置特殊形态 ->
    // `Class.member` 拆分。越靠前越确定，越靠后越保守。
    if let Some(target) = facade_qualified_pending_target(pending, source, nodes) {
        return Some(target);
    }

    if let Some(target) = by_name
        .get(pending.target_name.as_str())
        .and_then(|candidates| facade_pick_pending_target(pending, source, candidates.to_vec()))
    {
        return Some(target);
    }

    let normalized = pending
        .target_name
        .trim()
        .trim_matches(['"', '\''])
        .trim_start_matches('\\')
        .replace('\\', "::");
    if normalized.is_empty() {
        return None;
    }

    if pending.kind == EdgeKind::Calls {
        if let Some(target) = resolve_facade_member_call_target(&normalized, source, nodes)
            && facade_pending_target_allowed(pending, source, target)
        {
            return Some(target);
        }
        if let Some(member_name) = facade_react_native_member_name(&normalized, source.language)
            && let Some(target) = nodes
                .iter()
                .filter(|node| {
                    node.name == member_name
                        && matches!(node.kind, NodeKind::Function | NodeKind::Method)
                        && matches!(
                            node.language,
                            Language::ObjC | Language::Java | Language::Kotlin | Language::Cpp
                        )
                })
                .find(|node| node.language == Language::ObjC)
                .or_else(|| {
                    nodes.iter().find(|node| {
                        node.name == member_name
                            && matches!(node.kind, NodeKind::Function | NodeKind::Method)
                            && matches!(
                                node.language,
                                Language::Java | Language::Kotlin | Language::Cpp
                            )
                    })
                })
        {
            return Some(target);
        }
    }

    if pending.kind == EdgeKind::References {
        if let Some(prefix) = normalized.strip_suffix(":prefix") {
            let relaxed_prefix = facade_relaxed_config_key(prefix);
            if let Some(target) = nodes
                .iter()
                .filter(|node| matches!(node.language, Language::Yaml | Language::Properties))
                .filter(|node| {
                    facade_relaxed_config_key(&node.qualified_name).starts_with(&relaxed_prefix)
                })
                .filter(|node| facade_pending_target_allowed(pending, source, node))
                .min_by_key(|node| node.qualified_name.len())
            {
                return Some(target);
            }
        }
        let relaxed = facade_relaxed_config_key(&normalized);
        if let Some(target) = nodes.iter().find(|node| {
            (node.qualified_name == normalized
                || facade_relaxed_config_key(&node.qualified_name) == relaxed)
                && facade_pending_target_allowed(pending, source, node)
        }) {
            return Some(target);
        }
    }

    let (class_name, member_name) = facade_split_qualified_member(&normalized);
    let member_name = member_name?;
    facade_pick_pending_target(
        pending,
        source,
        nodes
            .iter()
            .filter(|node| node.name == member_name)
            .filter(|node| {
                facade_edge_target_matches_kind(node, pending)
                    && class_name.as_ref().is_none_or(|class_name| {
                        node.qualified_name.contains(class_name)
                            || node.file_path.contains(class_name)
                    })
            })
            .collect(),
    )
}

pub(super) fn facade_qualified_pending_target<'a>(
    pending: &RichFacadePendingEdge,
    source: &Node,
    nodes: &'a [Node],
) -> Option<&'a Node> {
    for variant in facade_qualified_target_variants(&pending.target_name, source.language) {
        let candidates = nodes
            .iter()
            .filter(|node| node.qualified_name == variant)
            .collect::<Vec<_>>();
        if let Some(target) = facade_pick_pending_target(pending, source, candidates) {
            return Some(target);
        }
    }
    None
}

pub(super) fn facade_qualified_target_variants(
    target_name: &str,
    language: Language,
) -> Vec<String> {
    let trimmed = target_name
        .trim()
        .trim_matches(['"', '\''])
        .trim_start_matches('\\');
    let mut variants = Vec::new();
    if !trimmed.is_empty() {
        variants.push(trimmed.to_owned());
    }
    if trimmed.contains('\\') {
        variants.push(trimmed.replace('\\', "::"));
        if language == Language::Php
            && let Some((namespace, name)) = trimmed.rsplit_once('\\')
            && !namespace.is_empty()
            && !name.is_empty()
        {
            variants.push(format!("{namespace}::{name}"));
        }
    }
    dedupe_names(variants)
}

pub(super) fn facade_pick_pending_target<'a>(
    pending: &RichFacadePendingEdge,
    source: &Node,
    candidates: Vec<&'a Node>,
) -> Option<&'a Node> {
    candidates
        .into_iter()
        .filter(|node| facade_pending_target_allowed(pending, source, node))
        .max_by_key(|node| facade_pending_target_score(pending, source, node))
}

pub(super) fn facade_pending_target_allowed(
    pending: &RichFacadePendingEdge,
    source: &Node,
    target: &Node,
) -> bool {
    // 语言家族过滤是为了防止同名符号跨生态误连；imports/calls 放宽到“未知家族不
    // 冲突”，让混合项目和桥接代码仍有机会解析。
    if target.id == source.id || !facade_edge_target_matches_kind(target, pending) {
        return false;
    }
    match pending.kind {
        EdgeKind::References => {
            matches!(target.language, Language::Yaml | Language::Properties)
                || (source.language == Language::Yaml
                    && (pending.target_name.contains('\\') || pending.target_name.contains("::")))
                || same_language_family(source.language, target.language)
        }
        EdgeKind::Extends | EdgeKind::Implements | EdgeKind::TypeOf | EdgeKind::Returns => {
            same_language_family(source.language, target.language)
        }
        EdgeKind::Imports | EdgeKind::Calls | EdgeKind::Instantiates => {
            !crosses_known_family(source.language, target.language)
        }
        _ => true,
    }
}

pub(super) fn facade_pending_target_score(
    pending: &RichFacadePendingEdge,
    source: &Node,
    target: &Node,
) -> (u8, u8, u8, usize, u8, std::cmp::Reverse<usize>) {
    // max_by_key 按 tuple 字典序选最可信目标：相对路径命中 > 同目录 > 文件后缀
    // > 共同目录前缀 > 同语言家族 > 更短路径。
    let import_target = pending.kind == EdgeKind::Imports;
    let relative_target = import_target
        .then(|| facade_source_relative_target(source, &pending.target_name))
        .flatten();
    let exact_relative = relative_target
        .as_ref()
        .is_some_and(|path| path == &target.file_path);
    let clean_target = facade_clean_import_target(&pending.target_name);
    let suffix_match = !clean_target.is_empty()
        && (target.file_path == clean_target
            || target.file_path.ends_with(&format!("/{clean_target}")));
    let same_dir = facade_dir(&source.file_path) == facade_dir(&target.file_path);
    (
        exact_relative as u8,
        same_dir as u8,
        suffix_match as u8,
        facade_shared_prefix_dirs(&source.file_path, &target.file_path),
        same_language_family(source.language, target.language) as u8,
        std::cmp::Reverse(target.file_path.len()),
    )
}

pub(super) fn facade_source_relative_target(source: &Node, target_name: &str) -> Option<String> {
    let target = facade_clean_import_target(target_name);
    if target.is_empty() || target.starts_with('/') || target.contains("://") {
        return None;
    }
    let dir = facade_dir(&source.file_path);
    Some(
        if dir.is_empty() {
            target
        } else {
            format!("{dir}/{target}")
        }
        .replace('\\', "/"),
    )
}

pub(super) fn facade_clean_import_target(target_name: &str) -> String {
    target_name
        .trim()
        .trim_matches(['"', '\''])
        .trim_start_matches("./")
        .to_owned()
}

pub(super) fn facade_dir(path: &str) -> &str {
    path.rsplit_once('/').map(|(dir, _)| dir).unwrap_or("")
}

pub(super) fn facade_shared_prefix_dirs(a: &str, b: &str) -> usize {
    a.split('/')
        .zip(b.split('/'))
        .take_while(|(left, right)| left == right)
        .count()
}

pub(super) fn facade_edge_target_matches_kind(
    node: &Node,
    pending: &RichFacadePendingEdge,
) -> bool {
    match pending.kind {
        EdgeKind::Calls => matches!(node.kind, NodeKind::Function | NodeKind::Method),
        EdgeKind::Instantiates => matches!(node.kind, NodeKind::Class | NodeKind::Struct),
        EdgeKind::References
            if pending
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get("fnRef"))
                .and_then(Value::as_bool)
                == Some(true) =>
        {
            matches!(node.kind, NodeKind::Function | NodeKind::Method)
        }
        EdgeKind::TypeOf | EdgeKind::Returns => facade_supertype_bearing(node.kind),
        _ => true,
    }
}
