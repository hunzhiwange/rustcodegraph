//! Override and platform-dispatch synthesis.
//!
//! 这组 pass 把“框架/runtime 会回调实现方法”的关系补成可遍历边，例如
//! setState->build、base virtual->override、expect->actual。

use std::collections::{HashMap, HashSet};

use serde_json::json;

use crate::db::queries::QueryBuilder;
use crate::resolution::types::ResolutionContext;
use crate::types::{Edge, EdgeKind, Language, Node, NodeKind};

use super::common::{
    MAX_CALLBACKS_PER_CHANNEL, children_of_kind, edge, semantic_duplicate_methods, slice_lines,
};

pub(super) fn flutter_build_edges(
    queries: &mut QueryBuilder,
    ctx: &mut dyn ResolutionContext,
) -> Vec<Edge> {
    // Flutter 中 setState 触发同一 widget/state class 的 build；这是 UI flow 的
    // 关键 hop，但只在同类 children 内连接。
    let mut edges = Vec::new();
    let mut seen = HashSet::new();
    for cls in queries
        .get_nodes_by_kind(NodeKind::Class)
        .unwrap_or_default()
    {
        let children = children_of_kind(queries, &cls.id, NodeKind::Method);
        let Some(build) = children
            .iter()
            .find(|node| node.name == "build" && node.file_path.ends_with(".dart"))
            .cloned()
        else {
            continue;
        };
        for method in children.into_iter().take(MAX_CALLBACKS_PER_CHANNEL) {
            if method.id == build.id {
                continue;
            }
            let src = ctx
                .read_file(&method.file_path)
                .and_then(|c| slice_lines(&c, method.start_line, method.end_line));
            if !src
                .as_deref()
                .map(|s| s.contains("setState("))
                .unwrap_or(false)
            {
                continue;
            }
            let key = format!("{}>{}", method.id, build.id);
            if !seen.insert(key) {
                continue;
            }
            edges.push(edge(
                &method.id,
                &build.id,
                EdgeKind::Calls,
                Some(method.start_line),
                "flutter-build",
                [
                    ("via", json!("setState")),
                    (
                        "registeredAt",
                        json!(format!("{}:{}", build.file_path, build.start_line)),
                    ),
                ],
            ));
        }
    }
    edges
}

pub(super) fn cpp_override_edges(queries: &mut QueryBuilder) -> Vec<Edge> {
    // C++ virtual dispatch 从 base method 走到子类同名 method。已有 extends 边提供
    // 继承边界，避免只按全仓同名匹配。
    let mut edges = Vec::new();
    let mut seen = HashSet::new();
    for cls in queries
        .get_nodes_by_kind(NodeKind::Class)
        .unwrap_or_default()
    {
        let sub_methods = children_of_kind(queries, &cls.id, NodeKind::Method)
            .into_iter()
            .filter(|node| node.language == Language::Cpp)
            .collect::<Vec<_>>();
        if sub_methods.is_empty() {
            continue;
        }
        for ext in queries
            .get_outgoing_edges(&cls.id, Some(vec![EdgeKind::Extends]), None)
            .unwrap_or_default()
        {
            let Some(base) = queries.get_node_by_id(&ext.target).unwrap_or(None) else {
                continue;
            };
            if base.language != Language::Cpp || base.id == cls.id {
                continue;
            }
            let base_methods = children_of_kind(queries, &base.id, NodeKind::Method)
                .into_iter()
                .map(|node| (node.name.clone(), node))
                .collect::<HashMap<_, _>>();
            for method in sub_methods.iter().take(MAX_CALLBACKS_PER_CHANNEL) {
                let Some(base_method) = base_methods.get(&method.name) else {
                    continue;
                };
                if base_method.id == method.id {
                    continue;
                }
                let key = format!("{}>{}", base_method.id, method.id);
                if !seen.insert(key) {
                    continue;
                }
                edges.push(edge(
                    &base_method.id,
                    &method.id,
                    EdgeKind::Calls,
                    Some(base_method.start_line),
                    "cpp-override",
                    [
                        ("via", json!(method.name)),
                        (
                            "registeredAt",
                            json!(format!("{}:{}", method.file_path, method.start_line)),
                        ),
                    ],
                ));
            }
        }
    }
    edges
}

pub(super) fn interface_override_edges(queries: &mut QueryBuilder) -> Vec<Edge> {
    // 多语言接口/trait 实现的统一桥接。source 可能有 semantic duplicate，占位和
    // 声明都连到实现，保证从抽象入口能继续走。
    let iface_langs = HashSet::from([
        Language::Java,
        Language::Kotlin,
        Language::CSharp,
        Language::TypeScript,
        Language::JavaScript,
        Language::Swift,
        Language::Scala,
        Language::Go,
        Language::Rust,
    ]);
    let mut edges = Vec::new();
    let mut seen = HashSet::new();
    for kind in [NodeKind::Class, NodeKind::Struct] {
        for cls in queries.get_nodes_by_kind(kind).unwrap_or_default() {
            let impl_methods = children_of_kind(queries, &cls.id, NodeKind::Method)
                .into_iter()
                .filter(|node| iface_langs.contains(&node.language))
                .collect::<Vec<_>>();
            if impl_methods.is_empty() {
                continue;
            }
            for sup in queries
                .get_outgoing_edges(
                    &cls.id,
                    Some(vec![EdgeKind::Implements, EdgeKind::Extends]),
                    None,
                )
                .unwrap_or_default()
            {
                let Some(base) = queries.get_node_by_id(&sup.target).unwrap_or(None) else {
                    continue;
                };
                if !iface_langs.contains(&base.language) || base.id == cls.id {
                    continue;
                }
                let mut impl_by_name: HashMap<String, Vec<Node>> = HashMap::new();
                for method in &impl_methods {
                    impl_by_name
                        .entry(method.name.clone())
                        .or_default()
                        .push(method.clone());
                }
                let mut added = 0;
                for base_method in children_of_kind(queries, &base.id, NodeKind::Method) {
                    if added >= MAX_CALLBACKS_PER_CHANNEL {
                        break;
                    }
                    let base_methods = semantic_duplicate_methods(queries, &base_method);
                    for method in impl_by_name.get(&base_method.name).into_iter().flatten() {
                        if added >= MAX_CALLBACKS_PER_CHANNEL || base_method.id == method.id {
                            continue;
                        }
                        for source_method in &base_methods {
                            if added >= MAX_CALLBACKS_PER_CHANNEL || source_method.id == method.id {
                                break;
                            }
                            let key = format!("{}>{}", source_method.id, method.id);
                            if !seen.insert(key) {
                                continue;
                            }
                            edges.push(edge(
                                &source_method.id,
                                &method.id,
                                EdgeKind::Calls,
                                Some(source_method.start_line),
                                "interface-impl",
                                [
                                    ("via", json!(method.name)),
                                    (
                                        "registeredAt",
                                        json!(format!(
                                            "{}:{}",
                                            method.file_path, method.start_line
                                        )),
                                    ),
                                ],
                            ));
                            added += 1;
                        }
                    }
                }
            }
        }
    }
    edges
}

pub(super) fn kotlin_expect_actual_edges(queries: &mut QueryBuilder) -> Vec<Edge> {
    // Kotlin Multiplatform 中 expect 声明和 actual 实现通常同 qualified_name 不同文件；
    // actual decorator 是精度锚点。
    let mut edges = Vec::new();
    let mut seen = HashSet::new();
    let actuals = queries
        .get_all_nodes()
        .unwrap_or_default()
        .into_iter()
        .filter(|node| {
            node.language == Language::Kotlin
                && node
                    .decorators
                    .as_ref()
                    .map(|d| d.iter().any(|x| x == "actual"))
                    .unwrap_or(false)
        })
        .collect::<Vec<_>>();
    for actual in actuals {
        for cand in queries
            .get_nodes_by_qualified_name_exact(&actual.qualified_name)
            .unwrap_or_default()
            .into_iter()
            .take(MAX_CALLBACKS_PER_CHANNEL)
        {
            if cand.language != Language::Kotlin
                || cand.id == actual.id
                || cand.file_path == actual.file_path
            {
                continue;
            }
            if cand
                .decorators
                .as_ref()
                .map(|d| d.iter().any(|x| x == "actual"))
                .unwrap_or(false)
            {
                continue;
            }
            if !kmp_kinds_compatible(cand.kind, actual.kind) {
                continue;
            }
            let key = format!("{}>{}", cand.id, actual.id);
            if seen.insert(key) {
                edges.push(edge(
                    &cand.id,
                    &actual.id,
                    EdgeKind::Calls,
                    Some(cand.start_line),
                    "kotlin-expect-actual",
                    [
                        ("via", json!(actual.name)),
                        (
                            "registeredAt",
                            json!(format!("{}:{}", actual.file_path, actual.start_line)),
                        ),
                    ],
                ));
            }
        }
    }
    edges
}

fn kmp_kinds_compatible(a: NodeKind, b: NodeKind) -> bool {
    // type-like 节点在不同平台上可能被抽成 class/interface/type_alias，允许同族匹配。
    let type_kinds = HashSet::from([
        NodeKind::Class,
        NodeKind::Interface,
        NodeKind::Struct,
        NodeKind::Enum,
        NodeKind::TypeAlias,
    ]);
    a == b || (type_kinds.contains(&a) && type_kinds.contains(&b))
}
