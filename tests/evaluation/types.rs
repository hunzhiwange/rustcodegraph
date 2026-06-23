use rustcodegraph::types::NodeKind;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EvalApi {
    #[serde(rename = "searchNodes")]
    SearchNodes,
    #[serde(rename = "findRelevantContext")]
    FindRelevantContext,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvalTestCase {
    pub id: String,
    pub query: String,
    pub api: EvalApi,
    pub expected_symbols: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kinds: Option<Vec<NodeKind>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<Map<String, Value>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvalResult {
    pub case_id: String,
    pub pass: bool,
    pub recall: f64,
    pub mrr: f64,
    pub found_symbols: Vec<String>,
    pub missed_symbols: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edge_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edge_density: Option<f64>,
    pub latency_ms: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvalReport {
    pub timestamp: String,
    pub codebase_path: String,
    pub codegraph_sha: String,
    pub summary: EvalSummary,
    pub results: Vec<EvalResult>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvalSummary {
    pub total: u64,
    pub passed: u64,
    pub failed: u64,
    pub mean_recall: f64,
    #[serde(rename = "meanMRR")]
    pub mean_mrr: f64,
}

#[test]
fn eval_test_case_preserves_search_nodes_shape() {
    let case = EvalTestCase {
        id: "search-class-exact".to_owned(),
        query: "TransportService".to_owned(),
        api: EvalApi::SearchNodes,
        expected_symbols: vec!["TransportService".to_owned()],
        kinds: Some(vec![NodeKind::Class]),
        options: Some(Map::from_iter([
            ("limit".to_owned(), json!(10)),
            ("caseSensitive".to_owned(), json!(false)),
        ])),
    };

    let serialized = serde_json::to_value(&case).expect("EvalTestCase should serialize");

    assert_eq!(
        serialized,
        json!({
            "id": "search-class-exact",
            "query": "TransportService",
            "api": "searchNodes",
            "expectedSymbols": ["TransportService"],
            "kinds": ["class"],
            "options": {
                "caseSensitive": false,
                "limit": 10,
            },
        })
    );

    let round_tripped: EvalTestCase =
        serde_json::from_value(serialized).expect("EvalTestCase should deserialize");

    assert_eq!(round_tripped, case);
}

#[test]
fn eval_test_case_preserves_find_relevant_context_shape() {
    let parsed: EvalTestCase = serde_json::from_value(json!({
        "id": "explore-rest-layer",
        "query": "How does the REST layer handle HTTP requests?",
        "api": "findRelevantContext",
        "expectedSymbols": ["RestController", "RestHandler", "BaseRestHandler", "RestRequest"],
        "options": {
            "searchLimit": 8,
            "traversalDepth": 3,
            "maxNodes": 80,
            "minScore": 0.2,
        },
    }))
    .expect("EvalTestCase should parse findRelevantContext cases");

    assert_eq!(parsed.api, EvalApi::FindRelevantContext);
    assert_eq!(parsed.kinds, None);
    assert_eq!(
        parsed.expected_symbols,
        vec![
            "RestController".to_owned(),
            "RestHandler".to_owned(),
            "BaseRestHandler".to_owned(),
            "RestRequest".to_owned(),
        ]
    );
    assert_eq!(
        parsed
            .options
            .as_ref()
            .and_then(|options| options.get("minScore")),
        Some(&json!(0.2))
    );
}

#[test]
fn eval_result_omits_optional_graph_metrics_when_absent() {
    let result = EvalResult {
        case_id: "search-class-exact".to_owned(),
        pass: true,
        recall: 1.0,
        mrr: 1.0,
        found_symbols: vec!["TransportService".to_owned()],
        missed_symbols: Vec::new(),
        node_count: None,
        edge_count: None,
        edge_density: None,
        latency_ms: 3.5,
    };

    let serialized = serde_json::to_value(&result).expect("EvalResult should serialize");

    assert_eq!(
        serialized,
        json!({
            "caseId": "search-class-exact",
            "pass": true,
            "recall": 1.0,
            "mrr": 1.0,
            "foundSymbols": ["TransportService"],
            "missedSymbols": [],
            "latencyMs": 3.5,
        })
    );

    assert!(serialized.get("nodeCount").is_none());
    assert!(serialized.get("edgeCount").is_none());
    assert!(serialized.get("edgeDensity").is_none());
}

#[test]
fn eval_result_preserves_context_metrics_when_present() {
    let parsed: EvalResult = serde_json::from_value(json!({
        "caseId": "explore-rest-layer",
        "pass": true,
        "recall": 0.75,
        "mrr": 0,
        "foundSymbols": ["RestController", "RestHandler", "RestRequest"],
        "missedSymbols": ["BaseRestHandler"],
        "nodeCount": 42,
        "edgeCount": 57,
        "edgeDensity": 1.357142857,
        "latencyMs": 24.25,
    }))
    .expect("EvalResult should parse optional graph metrics");

    assert_eq!(parsed.node_count, Some(42));
    assert_eq!(parsed.edge_count, Some(57));
    assert_eq!(parsed.edge_density, Some(1.357142857));
    assert_eq!(parsed.latency_ms, 24.25);
}

#[test]
fn eval_report_preserves_summary_and_results_shape() {
    let report = EvalReport {
        timestamp: "2026-06-19T00:00:00.000Z".to_owned(),
        codebase_path: "/tmp/codebase".to_owned(),
        codegraph_sha: "abc1234".to_owned(),
        summary: EvalSummary {
            total: 1,
            passed: 1,
            failed: 0,
            mean_recall: 1.0,
            mean_mrr: 1.0,
        },
        results: vec![EvalResult {
            case_id: "search-class-exact".to_owned(),
            pass: true,
            recall: 1.0,
            mrr: 1.0,
            found_symbols: vec!["TransportService".to_owned()],
            missed_symbols: Vec::new(),
            node_count: None,
            edge_count: None,
            edge_density: None,
            latency_ms: 3.5,
        }],
    };

    let serialized = serde_json::to_value(&report).expect("EvalReport should serialize");

    assert_eq!(serialized["timestamp"], "2026-06-19T00:00:00.000Z");
    assert_eq!(serialized["codebasePath"], "/tmp/codebase");
    assert_eq!(serialized["codegraphSha"], "abc1234");
    assert_eq!(
        serialized["summary"],
        json!({
            "total": 1,
            "passed": 1,
            "failed": 0,
            "meanRecall": 1.0,
            "meanMRR": 1.0,
        })
    );
    assert_eq!(serialized["results"][0]["caseId"], "search-class-exact");

    let round_tripped: EvalReport =
        serde_json::from_value(serialized).expect("EvalReport should deserialize");

    assert_eq!(round_tripped, report);
}
