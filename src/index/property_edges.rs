//! 字段/属性类型到类型声明的补充引用边。
//!
//! fallback 抽取通常只能把字段签名存成字符串；这里在全项目节点集合上解析签名里的大写类型标识符，
//! 让“谁引用了这个类型”类问题不必再回读源码。

use super::*;

pub(super) fn resolve_facade_property_type_edges(nodes: &[Node]) -> Vec<Edge> {
    let type_targets = nodes
        .iter()
        .filter(|node| {
            matches!(
                node.kind,
                NodeKind::Class
                    | NodeKind::Struct
                    | NodeKind::Interface
                    | NodeKind::Trait
                    | NodeKind::Protocol
                    | NodeKind::Enum
                    | NodeKind::TypeAlias
            )
        })
        .fold(HashMap::<&str, Vec<&Node>>::new(), |mut acc, node| {
            acc.entry(node.name.as_str()).or_default().push(node);
            acc
        });

    let mut seen = HashSet::new();
    let mut edges = Vec::new();
    for node in nodes.iter().filter(|node| {
        matches!(node.kind, NodeKind::Property | NodeKind::Field)
            && node
                .signature
                .as_deref()
                .is_some_and(|signature| !signature.is_empty())
    }) {
        let Some(signature) = node.signature.as_deref() else {
            continue;
        };
        for target_name in type_identifiers(signature) {
            let Some(candidates) = type_targets.get(target_name.as_str()) else {
                continue;
            };
            for target in candidates {
                if target.id == node.id || !same_language_family(target.language, node.language) {
                    continue;
                }
                let key = format!("{}|{}|{:?}", node.id, target.id, EdgeKind::References);
                if seen.insert(key) {
                    edges.push(Edge {
                        source: node.id.clone(),
                        target: target.id.clone(),
                        kind: EdgeKind::References,
                        metadata: None,
                        line: Some(node.start_line),
                        column: Some(node.start_column),
                        provenance: None,
                    });
                }
            }
        }
    }
    edges
}
