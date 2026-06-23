//! facade pending 边的成员解析辅助。
//!
//! 这里把 `foo.bar()`、`this.foo`、`Type::method` 等文本引用解析到具体节点；遇到多义候选时优先返回
//! None，避免生成看似完整但方向错误的结构边。

use super::*;

pub(super) fn resolve_facade_member_call_target<'a>(
    reference_name: &str,
    source: &Node,
    nodes: &'a [Node],
) -> Option<&'a Node> {
    let (receiver, method) = reference_name.rsplit_once('.')?;
    if receiver.is_empty() || method.is_empty() {
        return None;
    }
    let field_name = receiver.rsplit('.').next().unwrap_or(receiver);
    let source_prefix = facade_source_type_prefix(source)?;
    let source_type = facade_last_qualified_segment(&source_prefix);
    // 成员调用先根据当前类型上的字段签名找 receiver 类型，再从该类型中挑方法。
    let field = nodes
        .iter()
        .filter(|node| {
            let same_cpp_type = matches!(source.language, Language::C | Language::Cpp)
                && facade_enclosing_type_name(&node.qualified_name).as_deref()
                    == Some(source_type.as_str());
            matches!(node.kind, NodeKind::Property | NodeKind::Field)
                && node.name == field_name
                && (node.file_path == source.file_path || same_cpp_type)
                && same_language_family(node.language, source.language)
        })
        .find(|node| {
            facade_qualified_child_matches(&node.qualified_name, &source_prefix, field_name)
                || facade_enclosing_type_name(&node.qualified_name)
                    == Some(facade_last_qualified_segment(&source_prefix))
        })
        .or_else(|| {
            nodes.iter().find(|node| {
                let same_cpp_type = matches!(source.language, Language::C | Language::Cpp)
                    && facade_enclosing_type_name(&node.qualified_name).as_deref()
                        == Some(source_type.as_str());
                matches!(node.kind, NodeKind::Property | NodeKind::Field)
                    && node.name == field_name
                    && (node.file_path == source.file_path || same_cpp_type)
                    && same_language_family(node.language, source.language)
            })
        })?;
    let signature = field
        .signature
        .as_deref()
        .or(field.return_type.as_deref())?;
    let type_names = type_identifiers(signature);
    let type_name = type_names.first()?;
    let candidates = nodes
        .iter()
        .filter(|node| {
            matches!(node.kind, NodeKind::Method | NodeKind::Function)
                && node.name == method
                && same_language_family(node.language, source.language)
                && node.id != source.id
        })
        .collect::<Vec<_>>();
    let matching = candidates
        .iter()
        .copied()
        .filter(|node| {
            facade_enclosing_type_name(&node.qualified_name).as_deref() == Some(type_name.as_str())
                || node
                    .qualified_name
                    .ends_with(&format!("{type_name}::{method}"))
                || node
                    .qualified_name
                    .ends_with(&format!("{type_name}.{method}"))
        })
        .collect::<Vec<_>>();
    if !matching.is_empty() {
        if matches!(source.language, Language::C | Language::Cpp) {
            // C++ 可能同时有声明和定义节点；优先带限定名、带函数体、同文件的定义。
            return matching.into_iter().max_by_key(|node| {
                (
                    node.qualified_name
                        .ends_with(&format!("{type_name}::{method}")) as u8,
                    node.signature
                        .as_deref()
                        .is_some_and(|signature| signature.contains('{')) as u8,
                    (node.file_path == source.file_path) as u8,
                )
            });
        }
        return matching.into_iter().next();
    }
    if candidates.len() == 1 {
        candidates.first().copied()
    } else {
        None
    }
}

pub(super) fn facade_react_native_member_name(
    reference_name: &str,
    language: Language,
) -> Option<&str> {
    if !matches!(
        language,
        Language::TypeScript | Language::Tsx | Language::JavaScript | Language::Jsx
    ) || !reference_name.starts_with("NativeModules.")
    {
        return None;
    }
    reference_name
        .rsplit('.')
        .next()
        .filter(|name| !name.is_empty())
}

pub(super) fn facade_split_qualified_member(input: &str) -> (Option<String>, Option<&str>) {
    if let Some((class, member)) = input.rsplit_once("::") {
        let class_name = class
            .rsplit("::")
            .next()
            .filter(|value| !value.is_empty())
            .map(str::to_owned);
        return (class_name, Some(member.trim_matches(':')));
    }
    if let Some((class, member)) = input.rsplit_once(':') {
        let class_name = class
            .rsplit([':', '.'])
            .next()
            .filter(|value| !value.is_empty())
            .map(str::to_owned);
        return (class_name, Some(member.trim_matches(':')));
    }
    if let Some((class, _)) = input.rsplit_once(".as_view") {
        return (None, class.rsplit('.').next());
    }
    (
        None,
        input.rsplit('.').next().filter(|value| !value.is_empty()),
    )
}

pub(super) fn facade_relaxed_config_key(input: &str) -> String {
    input
        .chars()
        .filter(|ch| *ch != '-' && *ch != '_' && *ch != '.')
        .flat_map(char::to_lowercase)
        .collect()
}

pub(super) fn facade_pending_is_fn_ref(edge: &RichFacadePendingEdge) -> bool {
    edge.kind == EdgeKind::References
        && edge
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("fnRef"))
            .and_then(Value::as_bool)
            == Some(true)
}

pub(super) fn facade_supertype_edges<'a>(
    nodes: &'a [Node],
    pending_edges: &[RichFacadePendingEdge],
    existing_edges: &[Edge],
    by_name: &HashMap<&str, Vec<&'a Node>>,
) -> HashMap<String, Vec<String>> {
    // 继承图同时吸收待解析边和已有直接边，供 this/super 成员解析沿父类型查找。
    let by_id = nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect::<HashMap<_, _>>();
    let mut out: HashMap<String, Vec<String>> = HashMap::new();
    for edge in pending_edges {
        if !matches!(edge.kind, EdgeKind::Extends | EdgeKind::Implements) {
            continue;
        }
        let Some(source) = by_id.get(edge.source.as_str()) else {
            continue;
        };
        if !facade_supertype_bearing(source.kind) {
            continue;
        }
        let Some(candidates) = by_name.get(edge.target_name.as_str()) else {
            continue;
        };
        let pool = candidates
            .iter()
            .copied()
            .filter(|node| {
                facade_supertype_bearing(node.kind)
                    && same_language_family(node.language, source.language)
            })
            .collect::<Vec<_>>();
        if pool.is_empty() {
            continue;
        }
        let same_file = pool
            .iter()
            .copied()
            .filter(|node| node.file_path == source.file_path)
            .collect::<Vec<_>>();
        let target = same_file.first().copied().or_else(|| {
            (pool.len() == 1)
                .then_some(pool[0])
                .or_else(|| pool.first().copied())
        });
        if let Some(target) = target {
            out.entry(source.id.clone())
                .or_default()
                .push(target.id.clone());
        }
    }
    for edge in existing_edges {
        if !matches!(edge.kind, EdgeKind::Extends | EdgeKind::Implements) {
            continue;
        }
        let (Some(source), Some(target)) = (
            by_id.get(edge.source.as_str()),
            by_id.get(edge.target.as_str()),
        ) else {
            continue;
        };
        if facade_supertype_bearing(source.kind)
            && facade_supertype_bearing(target.kind)
            && same_language_family(source.language, target.language)
        {
            out.entry(source.id.clone())
                .or_default()
                .push(target.id.clone());
        }
    }
    out
}

pub(super) fn resolve_facade_fn_ref(
    pending: &RichFacadePendingEdge,
    by_name: &HashMap<&str, Vec<&Node>>,
    by_id: &HashMap<&str, &Node>,
    supertypes: &HashMap<String, Vec<String>>,
) -> Option<Edge> {
    let source = by_id.get(pending.source.as_str()).copied()?;
    // fnRef 保留 references 语义：它描述可执行值的传递关系，而不是源节点立即调用目标。
    let target = if let Some(member) = pending.target_name.strip_prefix("this.") {
        resolve_facade_this_member(source, member, by_name, by_id, supertypes)?
    } else if pending.target_name.contains("::") {
        resolve_facade_scoped_fn_ref(source, &pending.target_name, by_name)?
    } else {
        resolve_facade_bare_fn_ref(source, &pending.target_name, by_name)?
    };
    if source.id == target.id {
        return None;
    }
    Some(Edge {
        source: pending.source.clone(),
        target: target.id.clone(),
        kind: EdgeKind::References,
        metadata: pending.metadata.clone(),
        line: pending.line,
        column: pending.column,
        provenance: None,
    })
}
