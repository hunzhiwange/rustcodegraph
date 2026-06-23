use std::collections::HashSet;

use rustcodegraph::types::{SearchResult, Subgraph};

use super::types::EvalResult;

const PASS_THRESHOLD: f64 = 0.5;

pub(super) fn score_search_nodes(
    case_id: &str,
    expected_symbols: &[&str],
    results: &[SearchResult],
    latency_ms: f64,
) -> EvalResult {
    let expected_lower = expected_symbols
        .iter()
        .map(|symbol| symbol.to_ascii_lowercase())
        .collect::<Vec<_>>();
    let result_names = results
        .iter()
        .map(|result| result.node.name.to_ascii_lowercase())
        .collect::<Vec<_>>();

    let mut found = Vec::new();
    let mut missed = Vec::new();
    let mut first_rank = 0usize;

    for (i, expected) in expected_lower.iter().enumerate() {
        if let Some(idx) = result_names.iter().position(|name| name == expected) {
            found.push(expected_symbols[i].to_owned());
            if first_rank == 0 {
                first_rank = idx + 1;
            }
        } else {
            missed.push(expected_symbols[i].to_owned());
        }
    }

    let recall = if expected_symbols.is_empty() {
        0.0
    } else {
        found.len() as f64 / expected_symbols.len() as f64
    };
    let mrr = if first_rank > 0 {
        1.0 / first_rank as f64
    } else {
        0.0
    };

    EvalResult {
        case_id: case_id.to_owned(),
        pass: recall >= PASS_THRESHOLD,
        recall,
        mrr,
        found_symbols: found,
        missed_symbols: missed,
        node_count: None,
        edge_count: None,
        edge_density: None,
        latency_ms,
    }
}

pub(super) fn score_find_relevant_context(
    case_id: &str,
    expected_symbols: &[&str],
    subgraph: &Subgraph,
    latency_ms: f64,
) -> EvalResult {
    let node_names = subgraph
        .nodes
        .values()
        .map(|node| node.name.to_ascii_lowercase())
        .collect::<HashSet<_>>();

    let mut found = Vec::new();
    let mut missed = Vec::new();

    for symbol in expected_symbols {
        if node_names.contains(&symbol.to_ascii_lowercase()) {
            found.push((*symbol).to_owned());
        } else {
            missed.push((*symbol).to_owned());
        }
    }

    let recall = if expected_symbols.is_empty() {
        0.0
    } else {
        found.len() as f64 / expected_symbols.len() as f64
    };
    let node_count = subgraph.nodes.len();
    let edge_count = subgraph.edges.len();
    let edge_density = if node_count > 0 {
        edge_count as f64 / node_count as f64
    } else {
        0.0
    };

    EvalResult {
        case_id: case_id.to_owned(),
        pass: recall >= PASS_THRESHOLD,
        recall,
        mrr: 0.0,
        found_symbols: found,
        missed_symbols: missed,
        node_count: Some(node_count),
        edge_count: Some(edge_count),
        edge_density: Some(edge_density),
        latency_ms,
    }
}
