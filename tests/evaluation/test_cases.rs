//! Rust port of `__tests__/evaluation/test-cases.ts`.
//!
//! The TypeScript source exports a fixture table rather than a Vitest suite, so
//! these tests lock down the exported cases one-to-one.

use rustcodegraph::types::NodeKind;
use serde_json::{Map, Value, json};

use super::types::{EvalApi, EvalTestCase};

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

fn options(entries: &[(&str, Value)]) -> Map<String, Value> {
    entries
        .iter()
        .map(|(key, value)| ((*key).to_owned(), value.clone()))
        .collect()
}

fn default_explore_options() -> Map<String, Value> {
    options(&[
        ("searchLimit", json!(8)),
        ("traversalDepth", json!(3)),
        ("maxNodes", json!(80)),
        ("minScore", json!(0.2)),
    ])
}

pub fn test_cases() -> Vec<EvalTestCase> {
    vec![
        // === searchNodes: Symbol Lookup Precision ===
        EvalTestCase {
            id: "search-class-exact".to_owned(),
            query: "TransportService".to_owned(),
            api: EvalApi::SearchNodes,
            expected_symbols: strings(&["TransportService"]),
            kinds: Some(vec![NodeKind::Class]),
            options: None,
        },
        EvalTestCase {
            id: "search-method-qualified".to_owned(),
            query: "TransportService sendRequest".to_owned(),
            api: EvalApi::SearchNodes,
            expected_symbols: strings(&["sendRequest"]),
            kinds: Some(vec![NodeKind::Method]),
            options: None,
        },
        EvalTestCase {
            id: "search-interface".to_owned(),
            query: "ActionListener".to_owned(),
            api: EvalApi::SearchNodes,
            expected_symbols: strings(&["ActionListener"]),
            kinds: Some(vec![NodeKind::Interface]),
            options: None,
        },
        EvalTestCase {
            id: "search-enum".to_owned(),
            query: "RestStatus".to_owned(),
            api: EvalApi::SearchNodes,
            expected_symbols: strings(&["RestStatus"]),
            kinds: Some(vec![NodeKind::Enum]),
            options: None,
        },
        EvalTestCase {
            id: "search-exception".to_owned(),
            query: "SearchPhaseExecutionException".to_owned(),
            api: EvalApi::SearchNodes,
            expected_symbols: strings(&["SearchPhaseExecutionException"]),
            kinds: Some(vec![NodeKind::Class]),
            options: None,
        },
        EvalTestCase {
            id: "search-nested-class".to_owned(),
            query: "Engine Index".to_owned(),
            api: EvalApi::SearchNodes,
            expected_symbols: strings(&["Index"]),
            kinds: Some(vec![NodeKind::Class]),
            options: None,
        },
        // === findRelevantContext: Exploration Quality ===
        EvalTestCase {
            id: "explore-rest-layer".to_owned(),
            query: "How does the REST layer handle HTTP requests?".to_owned(),
            api: EvalApi::FindRelevantContext,
            expected_symbols: strings(&[
                "RestController",
                "RestHandler",
                "BaseRestHandler",
                "RestRequest",
            ]),
            kinds: None,
            options: Some(default_explore_options()),
        },
        EvalTestCase {
            id: "explore-search-execution".to_owned(),
            query: "How does search execution work from request to shard?".to_owned(),
            api: EvalApi::FindRelevantContext,
            expected_symbols: strings(&[
                "ShardSearchRequest",
                "SearchShardsRequest",
                "SearchShardsGroup",
            ]),
            kinds: None,
            options: Some(default_explore_options()),
        },
        EvalTestCase {
            id: "explore-bulk-indexing".to_owned(),
            query: "How does bulk indexing work?".to_owned(),
            api: EvalApi::FindRelevantContext,
            expected_symbols: strings(&["TransportBulkAction", "BulkRequest", "BulkResponse"]),
            kinds: None,
            options: Some(default_explore_options()),
        },
        EvalTestCase {
            id: "explore-shard-allocation".to_owned(),
            query: "How does shard rebalancing and allocation work?".to_owned(),
            api: EvalApi::FindRelevantContext,
            expected_symbols: strings(&["AllocationService", "BalancedShardsAllocator"]),
            kinds: None,
            options: Some(default_explore_options()),
        },
        EvalTestCase {
            id: "explore-transport-search".to_owned(),
            query: "How does TransportService connect to SearchTransportService?".to_owned(),
            api: EvalApi::FindRelevantContext,
            expected_symbols: strings(&["TransportService", "SearchTransportService"]),
            kinds: None,
            options: Some(default_explore_options()),
        },
        EvalTestCase {
            id: "explore-engine-implementations".to_owned(),
            query: "What are the Engine implementations for indexing?".to_owned(),
            api: EvalApi::FindRelevantContext,
            expected_symbols: strings(&["InternalEngine", "ReadOnlyEngine", "Engine"]),
            kinds: None,
            options: Some(default_explore_options()),
        },
    ]
}

fn find_case(cases: &[EvalTestCase], id: &str) -> EvalTestCase {
    cases
        .iter()
        .find(|case| case.id == id)
        .cloned()
        .unwrap_or_else(|| panic!("missing evaluation case {id}"))
}

fn assert_case(
    case: EvalTestCase,
    query: &str,
    api: EvalApi,
    expected_symbols: &[&str],
    kinds: Option<Vec<NodeKind>>,
    options: Option<Map<String, Value>>,
) {
    assert_eq!(case.query, query);
    assert_eq!(case.api, api);
    assert_eq!(case.expected_symbols, strings(expected_symbols));
    assert_eq!(case.kinds, kinds);
    assert_eq!(case.options, options);
}

#[test]
fn preserves_all_evaluation_cases_in_source_order() {
    let ids = test_cases()
        .iter()
        .map(|case| case.id.clone())
        .collect::<Vec<_>>();
    assert_eq!(
        ids,
        strings(&[
            "search-class-exact",
            "search-method-qualified",
            "search-interface",
            "search-enum",
            "search-exception",
            "search-nested-class",
            "explore-rest-layer",
            "explore-search-execution",
            "explore-bulk-indexing",
            "explore-shard-allocation",
            "explore-transport-search",
            "explore-engine-implementations",
        ])
    );
}

#[test]
fn preserves_api_group_counts() {
    let cases = test_cases();
    let search_nodes = cases
        .iter()
        .filter(|case| case.api == EvalApi::SearchNodes)
        .count();
    let find_relevant_context = cases
        .iter()
        .filter(|case| case.api == EvalApi::FindRelevantContext)
        .count();

    assert_eq!(search_nodes, 6);
    assert_eq!(find_relevant_context, 6);
}

mod search_nodes_symbol_lookup_precision {
    use super::*;

    #[test]
    fn search_class_exact() {
        let cases = test_cases();
        assert_case(
            find_case(&cases, "search-class-exact"),
            "TransportService",
            EvalApi::SearchNodes,
            &["TransportService"],
            Some(vec![NodeKind::Class]),
            None,
        );
    }

    #[test]
    fn search_method_qualified() {
        let cases = test_cases();
        assert_case(
            find_case(&cases, "search-method-qualified"),
            "TransportService sendRequest",
            EvalApi::SearchNodes,
            &["sendRequest"],
            Some(vec![NodeKind::Method]),
            None,
        );
    }

    #[test]
    fn search_interface() {
        let cases = test_cases();
        assert_case(
            find_case(&cases, "search-interface"),
            "ActionListener",
            EvalApi::SearchNodes,
            &["ActionListener"],
            Some(vec![NodeKind::Interface]),
            None,
        );
    }

    #[test]
    fn search_enum() {
        let cases = test_cases();
        assert_case(
            find_case(&cases, "search-enum"),
            "RestStatus",
            EvalApi::SearchNodes,
            &["RestStatus"],
            Some(vec![NodeKind::Enum]),
            None,
        );
    }

    #[test]
    fn search_exception() {
        let cases = test_cases();
        assert_case(
            find_case(&cases, "search-exception"),
            "SearchPhaseExecutionException",
            EvalApi::SearchNodes,
            &["SearchPhaseExecutionException"],
            Some(vec![NodeKind::Class]),
            None,
        );
    }

    #[test]
    fn search_nested_class() {
        let cases = test_cases();
        assert_case(
            find_case(&cases, "search-nested-class"),
            "Engine Index",
            EvalApi::SearchNodes,
            &["Index"],
            Some(vec![NodeKind::Class]),
            None,
        );
    }
}

mod find_relevant_context_exploration_quality {
    use super::*;

    #[test]
    fn explore_rest_layer() {
        let cases = test_cases();
        assert_case(
            find_case(&cases, "explore-rest-layer"),
            "How does the REST layer handle HTTP requests?",
            EvalApi::FindRelevantContext,
            &[
                "RestController",
                "RestHandler",
                "BaseRestHandler",
                "RestRequest",
            ],
            None,
            Some(default_explore_options()),
        );
    }

    #[test]
    fn explore_search_execution() {
        let cases = test_cases();
        assert_case(
            find_case(&cases, "explore-search-execution"),
            "How does search execution work from request to shard?",
            EvalApi::FindRelevantContext,
            &[
                "ShardSearchRequest",
                "SearchShardsRequest",
                "SearchShardsGroup",
            ],
            None,
            Some(default_explore_options()),
        );
    }

    #[test]
    fn explore_bulk_indexing() {
        let cases = test_cases();
        assert_case(
            find_case(&cases, "explore-bulk-indexing"),
            "How does bulk indexing work?",
            EvalApi::FindRelevantContext,
            &["TransportBulkAction", "BulkRequest", "BulkResponse"],
            None,
            Some(default_explore_options()),
        );
    }

    #[test]
    fn explore_shard_allocation() {
        let cases = test_cases();
        assert_case(
            find_case(&cases, "explore-shard-allocation"),
            "How does shard rebalancing and allocation work?",
            EvalApi::FindRelevantContext,
            &["AllocationService", "BalancedShardsAllocator"],
            None,
            Some(default_explore_options()),
        );
    }

    #[test]
    fn explore_transport_search() {
        let cases = test_cases();
        assert_case(
            find_case(&cases, "explore-transport-search"),
            "How does TransportService connect to SearchTransportService?",
            EvalApi::FindRelevantContext,
            &["TransportService", "SearchTransportService"],
            None,
            Some(default_explore_options()),
        );
    }

    #[test]
    fn explore_engine_implementations() {
        let cases = test_cases();
        assert_case(
            find_case(&cases, "explore-engine-implementations"),
            "What are the Engine implementations for indexing?",
            EvalApi::FindRelevantContext,
            &["InternalEngine", "ReadOnlyEngine", "Engine"],
            None,
            Some(default_explore_options()),
        );
    }
}
