//! Java/Kotlin to MyBatis XML mapper synthesis.
//!
//! MyBatis 的 SQL statement 节点在 XML 中，调用入口在 Java/Kotlin mapper 方法。
//! 只有 namespace class + statement id 唯一匹配时才连边。

use std::collections::{HashMap, HashSet};

use serde_json::json;

use crate::db::queries::QueryBuilder;
use crate::types::{Edge, EdgeKind, Language, Node, NodeKind};

use super::common::edge;

pub(super) fn mybatis_java_xml_edges(queries: &mut QueryBuilder) -> Vec<Edge> {
    // 建一个 `Class::method` 索引，XML 的 namespace 最后一段通常就是 mapper class。
    let mut java_index: HashMap<String, Vec<Node>> = HashMap::new();
    for method in queries
        .get_nodes_by_kind(NodeKind::Method)
        .unwrap_or_default()
    {
        if !matches!(method.language, Language::Java | Language::Kotlin) {
            continue;
        }
        let parts = method.qualified_name.split("::").collect::<Vec<_>>();
        if parts.len() >= 2 {
            java_index
                .entry(format!(
                    "{}::{}",
                    parts[parts.len() - 2],
                    parts[parts.len() - 1]
                ))
                .or_default()
                .push(method);
        }
    }
    let mut edges = Vec::new();
    let mut seen = HashSet::new();
    for xml in queries
        .get_nodes_by_kind(NodeKind::Method)
        .unwrap_or_default()
    {
        if xml.language != Language::Xml {
            continue;
        }
        let Some((namespace, id)) = xml.qualified_name.rsplit_once("::") else {
            continue;
        };
        let class_name = namespace.rsplit('.').next().unwrap_or(namespace);
        let candidates = java_index.get(&format!("{class_name}::{id}"));
        if candidates.map(Vec::len) != Some(1) {
            // 重载/多模块同名 mapper 可能存在，唯一候选是这里的精度阈值。
            continue;
        }
        let java = &candidates.unwrap()[0];
        let key = format!("{}>{}", java.id, xml.id);
        if seen.insert(key) {
            edges.push(edge(
                &java.id,
                &xml.id,
                EdgeKind::Calls,
                Some(java.start_line),
                "mybatis-java-xml",
                [
                    ("via", json!(format!("{class_name}.{id}"))),
                    (
                        "registeredAt",
                        json!(format!("{}:{}", xml.file_path, xml.start_line)),
                    ),
                ],
            ));
        }
    }
    edges
}
