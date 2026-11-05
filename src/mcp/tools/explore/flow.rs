//! Flow and blast-radius sections for `rustcodegraph_explore`.
//!
//! 这里的目标是把 agent 已经给出的符号袋连接起来：优先展示短 call path，
//! 而不是做开放式全图寻路。

use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};

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

    let paths = named_flow_paths(cg, named_nodes);

    if paths.is_empty() {
        lines.push("- No static call path found among the queried symbols; source context is included below.".to_string());
        return lines;
    }

    for path in paths {
        lines.push(format!("- {}", format_flow_path(&path)));
    }
    lines
}

fn path_rank(
    path: &[FlowStep],
    named: &HashSet<String>,
) -> (usize, Reverse<usize>, Reverse<usize>) {
    let coverage = path
        .iter()
        .filter(|s| named.contains(&s.node.id))
        .map(|s| s.node.name.as_str())
        .collect::<HashSet<_>>()
        .len();
    let bridges = path.iter().filter(|s| !named.contains(&s.node.id)).count();
    (coverage, Reverse(bridges), Reverse(path.len()))
}

fn sort_flow_paths(paths: &mut [Vec<FlowStep>], named: &HashSet<String>) {
    paths.sort_by(|a, b| {
        path_rank(b, named).cmp(&path_rank(a, named)).then_with(|| {
            a.iter()
                .map(|s| &s.node.id)
                .cmp(b.iter().map(|s| &s.node.id))
        })
    });
}

fn named_flow_paths(cg: &mut crate::CodeGraph, named_nodes: &[Node]) -> Vec<Vec<FlowStep>> {
    // One bounded search for the symbol bag, rather than stopping at the first
    // three arbitrary source/target pairs. At most one unnamed bridge is allowed.
    let named = named_nodes
        .iter()
        .map(|n| n.id.clone())
        .collect::<HashSet<_>>();
    let mut frontier = named_nodes
        .iter()
        .map(|node| {
            vec![FlowStep {
                node: node.clone(),
                edge: None,
            }]
        })
        .collect::<Vec<_>>();
    sort_flow_paths(&mut frontier, &named);
    frontier.truncate(64);
    let mut adjacency: HashMap<String, Vec<(Node, Edge)>> = HashMap::new();
    let mut paths = Vec::new();
    let mut expansions = 0;
    for _ in 0..8 {
        let mut next = Vec::new();
        for path in frontier {
            if expansions >= 600 {
                break;
            }
            expansions += 1;
            let current = &path.last().unwrap().node.id;
            let outgoing = adjacency.entry(current.clone()).or_insert_with(|| {
                let mut edges = cg
                    .get_outgoing_edges(current)
                    .into_iter()
                    .filter(|e| is_flow_edge(e.kind))
                    .collect::<Vec<_>>();
                // A direct name-match may duplicate a registered runtime hop. Keep
                // the registration evidence when choosing between identical hops.
                let has_wiring = |e: &Edge| {
                    e.metadata.as_ref().is_some_and(|m| {
                        m.contains_key("synthesizedBy") && m.contains_key("registeredAt")
                    })
                };
                edges.sort_by(|a, b| {
                    a.target
                        .cmp(&b.target)
                        .then_with(|| has_wiring(b).cmp(&has_wiring(a)))
                });
                let mut seen = HashSet::new();
                edges.retain(|e| seen.insert((e.target.clone(), e.kind)));
                edges
                    .into_iter()
                    .filter_map(|e| cg.get_node(&e.target).map(|n| (n, e)))
                    .collect()
            });
            let bridges = path.iter().filter(|s| !named.contains(&s.node.id)).count();
            for (node, edge) in outgoing.iter() {
                if path.iter().any(|s| s.node.id == node.id)
                    || is_low_value_flow_node(node)
                    || (!named.contains(&node.id) && bridges >= 1)
                {
                    continue;
                }
                let mut extended = path.clone();
                extended.push(FlowStep {
                    node: node.clone(),
                    edge: Some(edge.clone()),
                });
                next.push(extended);
            }
        }
        sort_flow_paths(&mut next, &named);
        next.dedup_by(|a, b| {
            a.iter()
                .map(|s| &s.node.id)
                .eq(b.iter().map(|s| &s.node.id))
        });
        next.truncate(64);
        paths.extend(
            next.iter()
                .filter(|p| {
                    named.contains(&p.last().unwrap().node.id) && path_rank(p, &named).0 >= 2
                })
                .cloned(),
        );
        if next.is_empty() {
            break;
        }
        frontier = next;
    }
    sort_flow_paths(&mut paths, &named);
    let mut selected: Vec<Vec<FlowStep>> = Vec::new();
    for path in paths {
        // A full chain already explains each of its contiguous shorter segments.
        if selected.iter().any(|full| {
            full.windows(path.len()).any(|part| {
                part.iter()
                    .map(|s| &s.node.id)
                    .eq(path.iter().map(|s| &s.node.id))
            })
        }) {
            continue;
        }
        selected.push(path);
        if selected.len() == 3 {
            break;
        }
    }
    selected
}

pub(super) fn protected_flow_nodes(cg: &mut crate::CodeGraph, named_nodes: &[Node]) -> Vec<Node> {
    // Protect only the same bounded paths shown in Flow, with at most one
    // unnamed bridge. This is source allocation, not a broader graph search.
    let mut nodes = named_flow_paths(cg, named_nodes)
        .into_iter()
        .filter(|path| {
            path.iter()
                .filter(|step| !named_nodes.iter().any(|n| n.id == step.node.id))
                .count()
                <= 1
        })
        .flatten()
        .map(|step| step.node)
        .collect::<Vec<_>>();
    dedupe_nodes(&mut nodes);
    nodes
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
        let annotation = step
            .edge
            .as_ref()
            .and_then(|edge| edge.metadata.as_ref())
            .and_then(|metadata| {
                let mechanism = metadata.get("synthesizedBy")?.as_str()?;
                let wiring = metadata
                    .get("registeredAt")
                    .and_then(|v| v.as_str())
                    .map(|at| format!("; registeredAt: {at}"))
                    .unwrap_or_default();
                let transport =
                    if metadata.get("channel").and_then(|v| v.as_str()) == Some("socket") {
                        metadata
                            .get("tier")
                            .and_then(|v| v.as_str())
                            .map(|tier| format!("; socket {tier}"))
                            .unwrap_or_default()
                    } else if mechanism.starts_with("tauri-") {
                        metadata
                            .get("tier")
                            .and_then(|v| v.as_str())
                            .map(|tier| format!("; {tier}"))
                            .unwrap_or_default()
                    } else {
                        String::new()
                    };
                Some(format!(" [{mechanism}{transport}{wiring}]"))
            })
            .unwrap_or_default();
        rendered.push(format!(
            "--{via}{annotation}--> {}",
            format_flow_node(&step.node)
        ));
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
