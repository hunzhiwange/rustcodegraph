use rustcodegraph::types::NodeKind;

use super::types::{EvalApi, EvalOptions, EvalTestCase};

pub(super) fn original_eval_test_cases() -> Vec<EvalTestCase> {
    vec![
        // === searchNodes: Symbol Lookup Precision ===
        EvalTestCase {
            id: "search-class-exact",
            query: "TransportService",
            api: EvalApi::SearchNodes,
            expected_symbols: &["TransportService"],
            kinds: Some(vec![NodeKind::Class]),
            options: None,
        },
        EvalTestCase {
            id: "search-method-qualified",
            query: "TransportService sendRequest",
            api: EvalApi::SearchNodes,
            expected_symbols: &["sendRequest"],
            kinds: Some(vec![NodeKind::Method]),
            options: None,
        },
        EvalTestCase {
            id: "search-interface",
            query: "ActionListener",
            api: EvalApi::SearchNodes,
            expected_symbols: &["ActionListener"],
            kinds: Some(vec![NodeKind::Interface]),
            options: None,
        },
        EvalTestCase {
            id: "search-enum",
            query: "RestStatus",
            api: EvalApi::SearchNodes,
            expected_symbols: &["RestStatus"],
            kinds: Some(vec![NodeKind::Enum]),
            options: None,
        },
        EvalTestCase {
            id: "search-exception",
            query: "SearchPhaseExecutionException",
            api: EvalApi::SearchNodes,
            expected_symbols: &["SearchPhaseExecutionException"],
            kinds: Some(vec![NodeKind::Class]),
            options: None,
        },
        EvalTestCase {
            id: "search-nested-class",
            query: "Engine Index",
            api: EvalApi::SearchNodes,
            expected_symbols: &["Index"],
            kinds: Some(vec![NodeKind::Class]),
            options: None,
        },
        // === findRelevantContext: Exploration Quality ===
        EvalTestCase {
            id: "explore-rest-layer",
            query: "How does the REST layer handle HTTP requests?",
            api: EvalApi::FindRelevantContext,
            expected_symbols: &[
                "RestController",
                "RestHandler",
                "BaseRestHandler",
                "RestRequest",
            ],
            kinds: None,
            options: Some(EvalOptions {
                search_limit: Some(8),
                traversal_depth: Some(3),
                max_nodes: Some(80),
                min_score: Some(0.2),
                ..EvalOptions::default()
            }),
        },
        EvalTestCase {
            id: "explore-search-execution",
            query: "How does search execution work from request to shard?",
            api: EvalApi::FindRelevantContext,
            expected_symbols: &[
                "ShardSearchRequest",
                "SearchShardsRequest",
                "SearchShardsGroup",
            ],
            kinds: None,
            options: Some(EvalOptions {
                search_limit: Some(8),
                traversal_depth: Some(3),
                max_nodes: Some(80),
                min_score: Some(0.2),
                ..EvalOptions::default()
            }),
        },
        EvalTestCase {
            id: "explore-bulk-indexing",
            query: "How does bulk indexing work?",
            api: EvalApi::FindRelevantContext,
            expected_symbols: &["TransportBulkAction", "BulkRequest", "BulkResponse"],
            kinds: None,
            options: Some(EvalOptions {
                search_limit: Some(8),
                traversal_depth: Some(3),
                max_nodes: Some(80),
                min_score: Some(0.2),
                ..EvalOptions::default()
            }),
        },
        EvalTestCase {
            id: "explore-shard-allocation",
            query: "How does shard rebalancing and allocation work?",
            api: EvalApi::FindRelevantContext,
            expected_symbols: &["AllocationService", "BalancedShardsAllocator"],
            kinds: None,
            options: Some(EvalOptions {
                search_limit: Some(8),
                traversal_depth: Some(3),
                max_nodes: Some(80),
                min_score: Some(0.2),
                ..EvalOptions::default()
            }),
        },
        EvalTestCase {
            id: "explore-transport-search",
            query: "How does TransportService connect to SearchTransportService?",
            api: EvalApi::FindRelevantContext,
            expected_symbols: &["TransportService", "SearchTransportService"],
            kinds: None,
            options: Some(EvalOptions {
                search_limit: Some(8),
                traversal_depth: Some(3),
                max_nodes: Some(80),
                min_score: Some(0.2),
                ..EvalOptions::default()
            }),
        },
        EvalTestCase {
            id: "explore-engine-implementations",
            query: "What are the Engine implementations for indexing?",
            api: EvalApi::FindRelevantContext,
            expected_symbols: &["InternalEngine", "ReadOnlyEngine", "Engine"],
            kinds: None,
            options: Some(EvalOptions {
                search_limit: Some(8),
                traversal_depth: Some(3),
                max_nodes: Some(80),
                min_score: Some(0.2),
                ..EvalOptions::default()
            }),
        },
    ]
}
