//! Go gRPC generated-stub to implementation synthesis.
//!
//! `.pb.go`/`_grpc.pb.go` 里会有 Unimplemented* stub，真实业务实现通常在手写文件。
//! 当同名实现唯一时，把 stub 入口连到实现，帮助从 generated dispatcher 走到用户代码。

use std::collections::{HashMap, HashSet};

use serde_json::json;

use crate::db::queries::QueryBuilder;
use crate::types::{Edge, EdgeKind, Language, Node, NodeKind};

use super::common::edge;
use super::go::go_receiver_from_qualified_name;

pub(super) fn go_grpc_stub_impl_edges(queries: &mut QueryBuilder) -> Vec<Edge> {
    // 先按方法名收集非生成文件中的实现；同一语义位置去重，避免重复抽取节点。
    let methods = queries
        .get_nodes_by_kind(NodeKind::Method)
        .unwrap_or_default()
        .into_iter()
        .filter(|node| node.language == Language::Go)
        .collect::<Vec<_>>();
    let mut impls_by_name: HashMap<String, Vec<Node>> = HashMap::new();
    for method in &methods {
        if go_generated_file(&method.file_path) {
            continue;
        }
        let Some(receiver) = go_receiver_from_qualified_name(&method.qualified_name) else {
            continue;
        };
        if receiver.starts_with("Unimplemented") {
            continue;
        }
        let candidates = impls_by_name.entry(method.name.clone()).or_default();
        let key = go_method_semantic_key(method);
        if !candidates
            .iter()
            .any(|candidate| go_method_semantic_key(candidate) == key)
        {
            candidates.push(method.clone());
        }
    }

    let mut edges = Vec::new();
    let mut seen = HashSet::new();
    for stub in methods {
        if !go_generated_file(&stub.file_path) {
            continue;
        }
        let Some(receiver) = go_receiver_from_qualified_name(&stub.qualified_name) else {
            continue;
        };
        if !receiver.starts_with("Unimplemented") {
            continue;
        }
        if stub.name.starts_with("mustEmbed") || stub.name.starts_with("testEmbedded") {
            continue;
        }
        let Some(candidates) = impls_by_name.get(&stub.name) else {
            continue;
        };
        if candidates.len() != 1 {
            // 多个实现时无法静态判断具体 service，宁可不连。
            continue;
        }
        let target = &candidates[0];
        let key = format!("{}>{}", stub.id, target.id);
        if seen.insert(key) {
            edges.push(edge(
                &stub.id,
                &target.id,
                EdgeKind::Calls,
                Some(stub.start_line),
                "go-grpc-stub-impl",
                [
                    ("via", json!(format!("{}::{}", receiver, stub.name))),
                    (
                        "registeredAt",
                        json!(format!("{}:{}", stub.file_path, stub.start_line)),
                    ),
                ],
            ));
        }
    }
    edges
}

fn go_generated_file(file_path: &str) -> bool {
    file_path.ends_with(".pb.go") || file_path.ends_with("_grpc.pb.go")
}

fn go_method_semantic_key(method: &Node) -> String {
    format!(
        "{}:{}:{}:{}",
        method.file_path, method.qualified_name, method.start_line, method.end_line
    )
}
