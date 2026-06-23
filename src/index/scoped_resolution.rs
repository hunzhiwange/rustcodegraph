//! scoped/this/bare 函数引用的名称解析。
//!
//! 解析策略以高精度为先：同文件优先、重复定义需语义等价，多义候选宁可不连，避免误导 flow 查询。

use super::*;

pub(super) fn resolve_facade_this_member<'a>(
    source: &Node,
    member: &str,
    by_name: &HashMap<&str, Vec<&'a Node>>,
    by_id: &HashMap<&str, &'a Node>,
    supertypes: &HashMap<String, Vec<String>>,
) -> Option<&'a Node> {
    if member.is_empty() {
        return None;
    }
    let class_prefix = facade_source_type_prefix(source)?;
    // 先找当前类型自己的成员；只有找不到时才沿继承图向父类型扩展。
    let own = by_name
        .get(member)?
        .iter()
        .copied()
        .filter(|node| {
            matches!(node.kind, NodeKind::Function | NodeKind::Method)
                && same_language_family(node.language, source.language)
                && node.id != source.id
                && node.file_path == source.file_path
                && facade_qualified_child_matches(&node.qualified_name, &class_prefix, member)
        })
        .min_by_key(|node| node.start_line);
    if own.is_some() {
        return own;
    }

    let source_type_name = facade_last_qualified_segment(&class_prefix);
    let class_node = by_id
        .values()
        .copied()
        .find(|node| {
            facade_supertype_bearing(node.kind)
                && node.qualified_name == class_prefix
                && same_language_family(node.language, source.language)
        })
        .or_else(|| {
            by_id.values().copied().find(|node| {
                facade_supertype_bearing(node.kind)
                    && node.name == source_type_name
                    && same_language_family(node.language, source.language)
            })
        })?;
    let mut frontier = vec![class_node.id.clone()];
    let mut seen = HashSet::from([class_node.id.clone()]);
    for _depth in 0..5 {
        // 限制继承搜索深度，防止异常继承图或循环让 fallback 解析变成全图遍历。
        let mut next = Vec::new();
        for type_id in frontier {
            for super_id in supertypes.get(&type_id).into_iter().flatten() {
                if !seen.insert(super_id.clone()) {
                    continue;
                }
                let Some(super_node) = by_id.get(super_id.as_str()).copied() else {
                    continue;
                };
                let target = by_name
                    .get(member)
                    .into_iter()
                    .flatten()
                    .copied()
                    .filter(|node| {
                        matches!(node.kind, NodeKind::Function | NodeKind::Method)
                            && same_language_family(node.language, source.language)
                            && (facade_qualified_child_matches(
                                &node.qualified_name,
                                &super_node.qualified_name,
                                member,
                            ) || facade_enclosing_type_name(&node.qualified_name).as_deref()
                                == Some(super_node.name.as_str()))
                    })
                    .min_by_key(|node| node.start_line);
                if target.is_some() {
                    return target;
                }
                next.push(super_node.id.clone());
            }
        }
        if next.is_empty() {
            break;
        }
        frontier = next;
    }
    None
}

pub(super) fn resolve_facade_scoped_fn_ref<'a>(
    source: &Node,
    reference_name: &str,
    by_name: &HashMap<&str, Vec<&'a Node>>,
) -> Option<&'a Node> {
    // `Type::method` 需要 qualified_name 匹配；只在候选重复但语义等价时自动选一个。
    let member = reference_name.rsplit("::").next()?;
    let mut candidates = by_name
        .get(member)?
        .iter()
        .copied()
        .filter(|node| {
            matches!(node.kind, NodeKind::Function | NodeKind::Method)
                && same_language_family(node.language, source.language)
                && node.id != source.id
                && facade_scoped_qualified_matches(&node.qualified_name, reference_name)
        })
        .collect::<Vec<_>>();
    if candidates.is_empty() {
        return None;
    }
    let same_file = candidates
        .iter()
        .copied()
        .filter(|node| node.file_path == source.file_path)
        .collect::<Vec<_>>();
    if !same_file.is_empty() {
        candidates = same_file;
    }
    if candidates.len() > 1
        && matches!(source.language, Language::C | Language::Cpp)
        && facade_candidates_share_symbol_identity(&candidates)
    {
        return candidates.into_iter().min_by_key(|node| node.start_line);
    }
    if candidates.len() > 1 && !facade_candidates_are_semantic_duplicates(&candidates) {
        return None;
    }
    candidates.into_iter().min_by_key(|node| node.start_line)
}

pub(super) fn facade_candidates_share_symbol_identity(candidates: &[&Node]) -> bool {
    let Some(first) = candidates.first().copied() else {
        return false;
    };
    let first_enclosing = facade_enclosing_type_name(&first.qualified_name);
    candidates.iter().copied().all(|node| {
        node.file_path == first.file_path
            && node.name == first.name
            && node.kind == first.kind
            && facade_enclosing_type_name(&node.qualified_name) == first_enclosing
    })
}

pub(super) fn facade_candidates_are_semantic_duplicates(candidates: &[&Node]) -> bool {
    let Some(first) = candidates.first().copied() else {
        return false;
    };
    candidates.iter().copied().all(|node| {
        node.file_path == first.file_path
            && node.start_line == first.start_line
            && node.name == first.name
            && node.kind == first.kind
    })
}

pub(super) fn resolve_facade_bare_fn_ref<'a>(
    source: &Node,
    reference_name: &str,
    by_name: &HashMap<&str, Vec<&'a Node>>,
) -> Option<&'a Node> {
    // 对 JS/TS/Python/PHP/C++ 的裸名引用更保守，只匹配函数，避免把类方法误认为全局 callback。
    let bare_fn_only = matches!(
        source.language,
        Language::TypeScript
            | Language::Tsx
            | Language::JavaScript
            | Language::Jsx
            | Language::Cpp
            | Language::Python
            | Language::Php
    );
    let mut candidates = by_name
        .get(reference_name)?
        .iter()
        .copied()
        .filter(|node| {
            (node.kind == NodeKind::Function || (!bare_fn_only && node.kind == NodeKind::Method))
                && same_language_family(node.language, source.language)
                && node.id != source.id
        })
        .collect::<Vec<_>>();
    if candidates.is_empty() {
        return None;
    }

    if source.language == Language::Swift
        && candidates.iter().any(|node| node.kind == NodeKind::Method)
    {
        let class_prefix = facade_source_type_prefix(source);
        candidates.retain(|node| {
            if node.kind != NodeKind::Method {
                return true;
            }
            let Some(class_prefix) = &class_prefix else {
                return false;
            };
            let Some(sep) = node.qualified_name.rfind("::") else {
                return false;
            };
            let method_prefix = &node.qualified_name[..sep];
            method_prefix == class_prefix
                || method_prefix.ends_with(&format!("::{class_prefix}"))
                || class_prefix.ends_with(&format!("::{method_prefix}"))
        });
        if candidates.is_empty() {
            return None;
        }
    }

    let same_file = candidates
        .iter()
        .copied()
        .filter(|node| node.file_path == source.file_path)
        .collect::<Vec<_>>();
    if !same_file.is_empty() {
        if source.language == Language::Swift
            && same_file.len() > 1
            && same_file.iter().all(|node| node.kind == NodeKind::Method)
        {
            return None;
        }
        return same_file.into_iter().min_by_key(|node| node.start_line);
    }

    if candidates.len() > 1 && facade_candidates_are_semantic_duplicates(&candidates) {
        return candidates.into_iter().min_by_key(|node| node.start_line);
    }

    (candidates.len() == 1).then_some(candidates[0])
}

pub(super) fn facade_source_type_prefix(source: &Node) -> Option<String> {
    if facade_supertype_bearing(source.kind) || source.kind == NodeKind::Module {
        return Some(source.qualified_name.clone());
    }
    let sep = match (
        source.qualified_name.rfind("::"),
        source.qualified_name.rfind('.'),
    ) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    }?;
    (sep > 0).then(|| source.qualified_name[..sep].to_owned())
}

pub(super) fn facade_qualified_child_matches(
    qualified_name: &str,
    prefix: &str,
    member: &str,
) -> bool {
    qualified_name == format!("{prefix}::{member}")
        || qualified_name == format!("{prefix}.{member}")
}

pub(super) fn facade_scoped_qualified_matches(qualified_name: &str, reference_name: &str) -> bool {
    if qualified_name == reference_name || qualified_name.ends_with(&format!("::{reference_name}"))
    {
        return true;
    }
    let dotted = reference_name.replace("::", ".");
    qualified_name == dotted || qualified_name.ends_with(&format!(".{dotted}"))
}

pub(super) fn facade_last_qualified_segment(input: &str) -> String {
    input
        .rsplit([':', '.'])
        .find(|part| !part.is_empty())
        .unwrap_or(input)
        .to_owned()
}

pub(super) fn facade_enclosing_type_name(qualified_name: &str) -> Option<String> {
    let sep = match (qualified_name.rfind("::"), qualified_name.rfind('.')) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    }?;
    Some(facade_last_qualified_segment(&qualified_name[..sep]))
}

pub(super) fn facade_supertype_bearing(kind: NodeKind) -> bool {
    matches!(
        kind,
        NodeKind::Class
            | NodeKind::Struct
            | NodeKind::Interface
            | NodeKind::Trait
            | NodeKind::Protocol
            | NodeKind::Enum
    )
}
