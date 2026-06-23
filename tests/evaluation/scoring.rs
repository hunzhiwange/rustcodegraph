//! Evaluation scoring helpers.
//!
//! Rust port of `__tests__/evaluation/scoring.ts`.
//!
//! The TypeScript source is a support module rather than a Vitest suite, so
//! there are no `describe`, `it`, or `test` blocks to mirror. The assertions
//! below exercise the exported helper behavior one-to-one.

use std::collections::{HashMap, HashSet};

use super::types::EvalResult;
use rustcodegraph::types::{SearchResult, Subgraph};

pub const PASS_THRESHOLD: f64 = 0.5;

pub fn score_search_nodes(
    case_id: &str,
    expected_symbols: &[&str],
    results: &[SearchResult],
    latency_ms: f64,
) -> EvalResult {
    let expected_lower = expected_symbols
        .iter()
        .map(|symbol| symbol.to_lowercase())
        .collect::<Vec<_>>();
    let result_names = results
        .iter()
        .map(|result| result.node.name.to_lowercase())
        .collect::<Vec<_>>();

    let mut found = Vec::new();
    let mut missed = Vec::new();
    let mut first_rank = 0usize;

    for (index, expected) in expected_lower.iter().enumerate() {
        if let Some(result_index) = result_names.iter().position(|name| name == expected) {
            found.push(expected_symbols[index].to_string());
            if first_rank == 0 {
                first_rank = result_index + 1;
            }
        } else {
            missed.push(expected_symbols[index].to_string());
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
        case_id: case_id.to_string(),
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

pub fn score_find_relevant_context(
    case_id: &str,
    expected_symbols: &[&str],
    subgraph: &Subgraph,
    latency_ms: f64,
) -> EvalResult {
    let _expected_lower = expected_symbols
        .iter()
        .map(|symbol| symbol.to_lowercase())
        .collect::<HashSet<_>>();
    let mut node_names = HashSet::<String>::new();

    for node in subgraph.nodes.values() {
        node_names.insert(node.name.to_lowercase());
    }

    let mut found = Vec::new();
    let mut missed = Vec::new();

    for symbol in expected_symbols {
        if node_names.contains(&symbol.to_lowercase()) {
            found.push((*symbol).to_string());
        } else {
            missed.push((*symbol).to_string());
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
        case_id: case_id.to_string(),
        pass: recall >= PASS_THRESHOLD,
        recall,
        mrr: 0.0,
        found_symbols: found,
        missed_symbols: missed,
        node_count: Some(node_count as u64),
        edge_count: Some(edge_count as u64),
        edge_density: Some(edge_density),
        latency_ms,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustcodegraph::types::{Edge, EdgeKind, Language, Node, NodeKind};

    fn node(name: &str) -> Node {
        Node {
            id: format!("node-{name}"),
            kind: NodeKind::Function,
            name: name.to_string(),
            qualified_name: name.to_string(),
            file_path: "fixture.ts".to_string(),
            language: Language::TypeScript,
            start_line: 1,
            end_line: 1,
            start_column: 0,
            end_column: 1,
            docstring: None,
            signature: None,
            visibility: None,
            is_exported: None,
            is_async: None,
            is_static: None,
            is_abstract: None,
            decorators: None,
            type_parameters: None,
            return_type: None,
            updated_at: 0,
        }
    }

    fn search_result(name: &str) -> SearchResult {
        SearchResult {
            node: node(name),
            score: 1.0,
            highlights: None,
        }
    }

    fn edge(source: &str, target: &str) -> Edge {
        Edge {
            source: source.to_string(),
            target: target.to_string(),
            kind: EdgeKind::References,
            metadata: None,
            line: None,
            column: None,
            provenance: None,
        }
    }

    fn subgraph(node_names: &[&str], edges: Vec<Edge>) -> Subgraph {
        let nodes = node_names
            .iter()
            .map(|name| (format!("node-{name}"), node(name)))
            .collect::<HashMap<_, _>>();

        Subgraph {
            nodes,
            edges,
            roots: Vec::new(),
            confidence: None,
        }
    }

    fn assert_f64_eq(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < f64::EPSILON,
            "expected {actual} to equal {expected}"
        );
    }

    #[test]
    fn score_search_nodes_finds_symbols_case_insensitively() {
        let results = vec![
            search_result("transportservice"),
            search_result("SendRequest"),
        ];

        let scored = score_search_nodes(
            "search-method-qualified",
            &["TransportService", "sendRequest"],
            &results,
            12.5,
        );

        assert_eq!(scored.case_id, "search-method-qualified");
        assert!(scored.pass);
        assert_f64_eq(scored.recall, 1.0);
        assert_f64_eq(scored.mrr, 1.0);
        assert_eq!(
            scored.found_symbols,
            vec!["TransportService".to_string(), "sendRequest".to_string()]
        );
        assert!(scored.missed_symbols.is_empty());
        assert_eq!(scored.node_count, None);
        assert_eq!(scored.edge_count, None);
        assert_eq!(scored.edge_density, None);
        assert_f64_eq(scored.latency_ms, 12.5);
    }

    #[test]
    fn score_search_nodes_tracks_missed_symbols_and_threshold_failure() {
        let results = vec![search_result("TransportService")];

        let scored = score_search_nodes(
            "search-partial",
            &["TransportService", "RestHandler", "BaseRestHandler"],
            &results,
            7.0,
        );

        assert!(!scored.pass);
        assert_f64_eq(scored.recall, 1.0 / 3.0);
        assert_f64_eq(scored.mrr, 1.0);
        assert_eq!(scored.found_symbols, vec!["TransportService".to_string()]);
        assert_eq!(
            scored.missed_symbols,
            vec!["RestHandler".to_string(), "BaseRestHandler".to_string()]
        );
    }

    #[test]
    fn score_search_nodes_uses_first_found_expected_symbol_for_mrr() {
        let results = vec![search_result("EarlierResult"), search_result("LaterResult")];

        let scored = score_search_nodes(
            "search-ordered-mrr",
            &["LaterResult", "EarlierResult"],
            &results,
            1.0,
        );

        assert!(scored.pass);
        assert_f64_eq(scored.recall, 1.0);
        assert_f64_eq(scored.mrr, 0.5);
    }

    #[test]
    fn score_search_nodes_handles_empty_expected_symbols_like_typescript() {
        let results = vec![search_result("TransportService")];

        let scored = score_search_nodes("search-empty", &[], &results, 3.0);

        assert!(!scored.pass);
        assert_f64_eq(scored.recall, 0.0);
        assert_f64_eq(scored.mrr, 0.0);
        assert!(scored.found_symbols.is_empty());
        assert!(scored.missed_symbols.is_empty());
    }

    #[test]
    fn score_find_relevant_context_scores_recall_counts_and_density() {
        let graph = subgraph(
            &["RestController", "resthandler", "Unrelated"],
            vec![
                edge("node-RestController", "node-resthandler"),
                edge("node-resthandler", "node-Unrelated"),
            ],
        );

        let scored = score_find_relevant_context(
            "explore-rest-layer",
            &["RestController", "RestHandler", "BaseRestHandler"],
            &graph,
            24.0,
        );

        assert_eq!(scored.case_id, "explore-rest-layer");
        assert!(scored.pass);
        assert_f64_eq(scored.recall, 2.0 / 3.0);
        assert_f64_eq(scored.mrr, 0.0);
        assert_eq!(
            scored.found_symbols,
            vec!["RestController".to_string(), "RestHandler".to_string()]
        );
        assert_eq!(scored.missed_symbols, vec!["BaseRestHandler".to_string()]);
        assert_eq!(scored.node_count, Some(3));
        assert_eq!(scored.edge_count, Some(2));
        assert_f64_eq(
            scored.edge_density.expect("edge density should be set"),
            2.0 / 3.0,
        );
        assert_f64_eq(scored.latency_ms, 24.0);
    }

    #[test]
    fn score_find_relevant_context_handles_empty_graph_density() {
        let graph = subgraph(&[], Vec::new());

        let scored =
            score_find_relevant_context("explore-empty", &["TransportService"], &graph, 0.0);

        assert!(!scored.pass);
        assert_f64_eq(scored.recall, 0.0);
        assert_eq!(scored.found_symbols, Vec::<String>::new());
        assert_eq!(scored.missed_symbols, vec!["TransportService".to_string()]);
        assert_eq!(scored.node_count, Some(0));
        assert_eq!(scored.edge_count, Some(0));
        assert_f64_eq(
            scored.edge_density.expect("edge density should be set"),
            0.0,
        );
    }
}
