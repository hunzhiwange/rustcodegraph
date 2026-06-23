//! Flow and blast-radius sections for `rustcodegraph_explore`.
//!
//! 这里的目标是把 agent 已经给出的符号袋连接起来：优先展示短 call path，
//! 而不是做开放式全图寻路。

use std::collections::{HashSet, VecDeque};

use super::super::shared::{
    dedupe_nodes, edge_kind_label, node_matches_symbol, sort_nodes_for_output,
};
use crate::types::{Edge, EdgeKind, Node, NodeKind};

pub(super) fn format_explore_blast_radius(
    cg: &mut crate::CodeGraph,
    nodes: &[Node],
) -> Vec<String> {
    let mut rows = Vec::new();
    let mut seen_symbols = HashSet::new();
    for node in nodes {
        if !matches!(
            node.kind,
            NodeKind::Function | NodeKind::Method | NodeKind::Class | NodeKind::Interface
        ) {
            continue;
        }
        if !seen_symbols.insert(node.id.clone()) {
            continue;
        }

        let callers = cg
            .get_incoming_edges(&node.id)
            .into_iter()
            .filter(|edge| {
                !matches!(
                    edge.kind,
                    EdgeKind::Contains | EdgeKind::Imports | EdgeKind::Exports
                )
            })
            .filter_map(|edge| cg.get_node(&edge.source))
            .collect::<Vec<_>>();
        if callers.is_empty() {
            continue;
        }

        let mut caller_labels = callers
            .iter()
            .map(|caller| format!("{} ({})", caller.name, caller.file_path))
            .collect::<Vec<_>>();
        caller_labels.sort();
        caller_labels.dedup();

        let mut tests = callers
            .iter()
            .map(|caller| caller.file_path.clone())
            .filter(|path| is_test_file_path(path))
            .collect::<Vec<_>>();
        tests.sort();
        tests.dedup();
        let tests_label = if tests.is_empty() {
            "no covering tests".to_string()
        } else {
            format!("tests: {}", tests.join(", "))
        };

        rows.push(format!(
            "- `{}`: {} caller(s): {}; {}",
            node.name,
            caller_labels.len(),
            caller_labels.join(", "),
            tests_label
        ));
    }

    if rows.is_empty() {
        Vec::new()
    } else {
        let mut lines = vec!["### Blast radius".to_string()];
        lines.extend(rows);
        lines
    }
}

pub(super) fn exact_named_query_nodes(nodes: &[Node], query_tokens: &HashSet<String>) -> Vec<Node> {
    let mut out = nodes
        .iter()
        .filter(|node| {
            is_dynamic_named_node(node)
                && query_tokens.iter().any(|token| {
                    node.name == *token
                        || node.qualified_name == *token
                        || node_matches_symbol(node, token)
                })
        })
        .cloned()
        .collect::<Vec<_>>();
    sort_nodes_for_output(&mut out);
    dedupe_nodes(&mut out);
    out
}

fn is_dynamic_named_node(node: &Node) -> bool {
    matches!(
        node.kind,
        NodeKind::Function | NodeKind::Method | NodeKind::Class | NodeKind::Component
    )
}

pub(super) fn has_static_flow_between_named(
    cg: &mut crate::CodeGraph,
    named_nodes: &[Node],
) -> bool {
    if named_nodes.len() < 2 {
        return false;
    }
    let targets = named_nodes
        .iter()
        .map(|node| node.id.clone())
        .collect::<HashSet<_>>();
    for seed in named_nodes {
        let mut seen = HashSet::from([seed.id.clone()]);
        let mut frontier = vec![seed.id.clone()];
        for _ in 0..6 {
            let mut next = Vec::new();
            for id in frontier {
                for edge in cg.get_outgoing_edges(&id) {
                    if edge.kind != EdgeKind::Calls {
                        continue;
                    }
                    if edge.target != seed.id && targets.contains(&edge.target) {
                        return true;
                    }
                    if seen.insert(edge.target.clone()) {
                        next.push(edge.target);
                    }
                }
            }
            if next.is_empty() {
                break;
            }
            frontier = next;
        }
    }
    false
}

#[derive(Clone)]
struct FlowStep {
    node: Node,
    edge: Option<Edge>,
}

pub(super) fn format_flow_section(
    cg: &mut crate::CodeGraph,
    named_nodes: &[Node],
    query_tokens: &HashSet<String>,
) -> Vec<String> {
    let mut lines = vec!["## Flow (call path among the symbols you queried)".to_string()];
    if named_nodes.len() < 2 {
        lines.push(format!("- {}", flow_hint_for_query(cg, query_tokens)));
        return lines;
    }

    let mut paths = Vec::new();
    let mut seen_paths = HashSet::new();
    for from in named_nodes {
        for to in named_nodes {
            if from.id == to.id {
                continue;
            }
            let Some(path) = find_named_flow_path(cg, from, &to.id, 8) else {
                continue;
            };
            let key = path
                .iter()
                .map(|step| step.node.id.as_str())
                .collect::<Vec<_>>()
                .join(">");
            if seen_paths.insert(key) {
                paths.push(path);
            }
            if paths.len() >= 3 {
                break;
            }
        }
        if paths.len() >= 3 {
            break;
        }
    }

    if paths.is_empty() {
        lines.push("- No static call path found among the queried symbols; source context is included below.".to_string());
        return lines;
    }

    for path in paths {
        lines.push(format!("- {}", format_flow_path(&path)));
    }
    lines
}

fn find_named_flow_path(
    cg: &mut crate::CodeGraph,
    from: &Node,
    target_id: &str,
    max_depth: usize,
) -> Option<Vec<FlowStep>> {
    // BFS 有深度和扩展上限，且过滤低价值节点；explore 输出宁可少给路径，
    // 也不能在 god-function 或 import/file 节点上发散。
    let mut visited = HashSet::from([from.id.clone()]);
    let mut queue = VecDeque::from([vec![FlowStep {
        node: from.clone(),
        edge: None,
    }]]);
    let mut expansions = 0usize;

    while let Some(path) = queue.pop_front() {
        let current = path.last()?.node.clone();
        if current.id == target_id {
            return Some(path);
        }
        if path.len().saturating_sub(1) >= max_depth {
            continue;
        }
        expansions += 1;
        if expansions > 600 {
            break;
        }

        let mut edges = cg
            .get_outgoing_edges(&current.id)
            .into_iter()
            .filter(|edge| is_flow_edge(edge.kind))
            .collect::<Vec<_>>();
        edges.sort_by(|a, b| a.target.cmp(&b.target));

        for edge in edges {
            if !visited.insert(edge.target.clone()) {
                continue;
            }
            let Some(next_node) = cg.get_node(&edge.target) else {
                continue;
            };
            if is_low_value_flow_node(&next_node) && next_node.id != target_id {
                continue;
            }
            let mut next_path = path.clone();
            next_path.push(FlowStep {
                node: next_node,
                edge: Some(edge),
            });
            queue.push_back(next_path);
        }
    }
    None
}

fn is_flow_edge(kind: EdgeKind) -> bool {
    // 除 calls 外纳入 references/instantiates/decorates/overrides，是为了覆盖
    // 框架和合成边带来的“可执行流”。
    matches!(
        kind,
        EdgeKind::Calls
            | EdgeKind::References
            | EdgeKind::Instantiates
            | EdgeKind::Decorates
            | EdgeKind::Overrides
    )
}

fn is_low_value_flow_node(node: &Node) -> bool {
    matches!(
        node.kind,
        NodeKind::File | NodeKind::Import | NodeKind::Export | NodeKind::Parameter
    )
}

fn format_flow_path(path: &[FlowStep]) -> String {
    let mut rendered = Vec::new();
    for (index, step) in path.iter().enumerate() {
        if index == 0 {
            rendered.push(format_flow_node(&step.node));
            continue;
        }
        let via = step
            .edge
            .as_ref()
            .map(|edge| edge_kind_label(edge.kind))
            .unwrap_or("edge");
        rendered.push(format!("--{via}--> {}", format_flow_node(&step.node)));
    }
    rendered.join(" ")
}

fn format_flow_node(node: &Node) -> String {
    format!("{} ({}:{})", node.name, node.file_path, node.start_line)
}

fn is_test_file_path(path: &str) -> bool {
    let normalized = path.replace('\\', "/").to_ascii_lowercase();
    normalized.contains(".test.")
        || normalized.contains(".spec.")
        || normalized.contains("/test/")
        || normalized.contains("/tests/")
}

fn flow_hint_for_query(cg: &mut crate::CodeGraph, query_tokens: &HashSet<String>) -> String {
    let mut callables = Vec::new();
    for token in query_tokens {
        let has_callable = cg.search_nodes(token, None).into_iter().any(|result| {
            result.node.name == *token
                && matches!(result.node.kind, NodeKind::Function | NodeKind::Method)
        });
        if has_callable {
            callables.push(token.clone());
        }
    }
    callables.sort();
    if callables.is_empty() {
        "named symbols gathered into the file context below".to_string()
    } else {
        callables.join(" -> ")
    }
}
