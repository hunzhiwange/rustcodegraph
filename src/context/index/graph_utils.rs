//! Small graph-shaping helpers shared by context search.
//!
//! search.rs 负责“找什么”，这里负责“预算不够时保留什么”。核心原则是先保住
//! query roots 与一跳邻居，再用稳定排序补齐，避免 context 输出在同一输入下漂移。

use std::collections::{HashMap, HashSet};

use crate::search::query_utils::is_test_file;
use crate::types::{Confidence, Edge, Node, NodeKind, Subgraph};

use super::options::ceil_div;

pub(super) fn insert_node(nodes: &mut HashMap<String, Node>, node: Node) {
    nodes.entry(node.id.clone()).or_insert(node);
}

pub(super) fn push_edge_unique(edges: &mut Vec<Edge>, edge: Edge) {
    // Edge 的 metadata/provenance 可能不同，但 context 只需要一条可走关系；
    // 以 source/target/kind 去重可避免同一关系在 Markdown 中重复出现。
    if !edges.iter().any(|existing| same_edge(existing, &edge)) {
        edges.push(edge);
    }
}

fn same_edge(a: &Edge, b: &Edge) -> bool {
    a.source == b.source && a.target == b.target && a.kind == b.kind
}

pub(super) fn is_type_hierarchy_kind(kind: NodeKind) -> bool {
    matches!(
        kind,
        NodeKind::Class
            | NodeKind::Interface
            | NodeKind::Struct
            | NodeKind::Trait
            | NodeKind::Protocol
    )
}

pub(super) fn trim_to_max_nodes(
    nodes: HashMap<String, Node>,
    edges: Vec<Edge>,
    roots: &[String],
    max_nodes: usize,
) -> (HashMap<String, Node>, Vec<Edge>) {
    if nodes.len() <= max_nodes {
        return (nodes, edges);
    }

    // 先从 roots 扩一圈，保证用户 query 命中的符号和直接关系优先留下。
    let mut priority_ids = HashSet::<String>::new();
    let mut priority_order = Vec::<String>::new();
    for root in roots {
        if priority_ids.insert(root.clone()) {
            priority_order.push(root.clone());
        }
    }
    for edge in &edges {
        if priority_ids.contains(&edge.source) && priority_ids.insert(edge.target.clone()) {
            priority_order.push(edge.target.clone());
        }
        if priority_ids.contains(&edge.target) && priority_ids.insert(edge.source.clone()) {
            priority_order.push(edge.source.clone());
        }
    }

    let mut final_nodes = HashMap::new();
    for id in priority_order {
        if final_nodes.len() >= max_nodes {
            break;
        }
        if let Some(node) = nodes.get(&id) {
            final_nodes.insert(id, node.clone());
        }
    }

    let mut remaining = nodes.values().cloned().collect::<Vec<_>>();
    remaining.sort_by_key(node_sort_key);
    // 预算仍有余量时再按文件/行号稳定补齐，避免 HashMap 迭代顺序影响结果。
    for node in remaining {
        if final_nodes.len() >= max_nodes {
            break;
        }
        final_nodes.entry(node.id.clone()).or_insert(node);
    }

    let final_edges = edges
        .into_iter()
        .filter(|edge| {
            final_nodes.contains_key(&edge.source) && final_nodes.contains_key(&edge.target)
        })
        .collect();
    (final_nodes, final_edges)
}

pub(super) fn apply_file_diversity_cap(
    final_nodes: &mut HashMap<String, Node>,
    roots: &mut Vec<String>,
    max_nodes: usize,
) {
    let max_per_file = 5usize.max(ceil_div(max_nodes.saturating_mul(20), 100));
    // 单个大文件的局部密度很容易吞掉全部 context；保留 roots 和类型/函数节点，
    // 把其余节点让给其他文件，以便 agent 看到更完整的周边结构。
    let mut file_counts = HashMap::<String, Vec<String>>::new();
    for (id, node) in final_nodes.iter() {
        file_counts
            .entry(node.file_path.clone())
            .or_default()
            .push(id.clone());
    }
    let root_set = roots.iter().cloned().collect::<HashSet<_>>();

    for (_, mut node_ids) in file_counts {
        if node_ids.len() <= max_per_file {
            continue;
        }
        node_ids.sort_by(|a, b| {
            let a_score = final_nodes
                .get(a)
                .map(|node| node_cap_priority(node, &root_set))
                .unwrap_or(0);
            let b_score = final_nodes
                .get(b)
                .map(|node| node_cap_priority(node, &root_set))
                .unwrap_or(0);
            b_score.cmp(&a_score).then(a.cmp(b))
        });
        for id in node_ids.into_iter().skip(max_per_file) {
            final_nodes.remove(&id);
            roots.retain(|root| root != &id);
        }
    }
}

pub(super) fn apply_non_production_cap(
    final_nodes: &mut HashMap<String, Node>,
    roots: &mut Vec<String>,
    max_nodes: usize,
    is_test_query: bool,
) {
    if is_test_query {
        return;
    }
    let max_non_prod = 3usize.max(ceil_div(max_nodes.saturating_mul(15), 100));
    // 普通产品问题中，测试文件常因命名详细而排名靠前；限制数量可减少误导。
    // 当 query 明确提到 test/spec 时完全跳过这个 cap。
    let mut non_prod_ids = final_nodes
        .iter()
        .filter(|(_, node)| is_test_file(&node.file_path))
        .map(|(id, _)| id.clone())
        .collect::<Vec<_>>();
    non_prod_ids.sort();
    if non_prod_ids.len() <= max_non_prod {
        return;
    }
    for id in non_prod_ids.into_iter().skip(max_non_prod) {
        final_nodes.remove(&id);
        roots.retain(|root| root != &id);
    }
}

fn node_cap_priority(node: &Node, root_set: &HashSet<String>) -> i32 {
    let root = if root_set.contains(&node.id) { 10 } else { 0 };
    let kind = match node.kind {
        NodeKind::Class
        | NodeKind::Interface
        | NodeKind::Struct
        | NodeKind::Trait
        | NodeKind::Protocol
        | NodeKind::Enum => 3,
        NodeKind::Method | NodeKind::Function => 1,
        NodeKind::Property | NodeKind::Field | NodeKind::Variable => 0,
        _ => 0,
    };
    root + kind
}

pub(super) fn sorted_node_ids(nodes: &HashMap<String, Node>) -> Vec<String> {
    let mut sorted = nodes.values().collect::<Vec<_>>();
    sorted.sort_by_key(|a| node_sort_key(a));
    sorted.into_iter().map(|node| node.id.clone()).collect()
}

pub(super) fn node_sort_key(node: &Node) -> (String, u64, String, String) {
    (
        node.file_path.clone(),
        node.start_line,
        node.name.clone(),
        node.id.clone(),
    )
}

pub(super) fn make_subgraph(
    nodes: HashMap<String, Node>,
    edges: Vec<Edge>,
    roots: Vec<String>,
    confidence: Option<Confidence>,
) -> Subgraph {
    Subgraph {
        nodes,
        edges,
        roots,
        confidence,
    }
}
