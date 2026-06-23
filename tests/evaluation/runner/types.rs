use std::path::PathBuf;

use rustcodegraph::types::NodeKind;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EvalApi {
    SearchNodes,
    FindRelevantContext,
}

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct EvalOptions {
    pub(super) limit: Option<u64>,
    pub(super) search_limit: Option<u64>,
    pub(super) traversal_depth: Option<u64>,
    pub(super) max_nodes: Option<u64>,
    pub(super) min_score: Option<f64>,
}

#[derive(Debug, Clone)]
pub(super) struct EvalTestCase {
    pub(super) id: &'static str,
    pub(super) query: &'static str,
    pub(super) api: EvalApi,
    pub(super) expected_symbols: &'static [&'static str],
    pub(super) kinds: Option<Vec<NodeKind>>,
    pub(super) options: Option<EvalOptions>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct EvalResult {
    pub(super) case_id: String,
    pub(super) pass: bool,
    pub(super) recall: f64,
    pub(super) mrr: f64,
    pub(super) found_symbols: Vec<String>,
    pub(super) missed_symbols: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) node_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) edge_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) edge_density: Option<f64>,
    pub(super) latency_ms: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct EvalSummary {
    pub(super) total: usize,
    pub(super) passed: usize,
    pub(super) failed: usize,
    pub(super) mean_recall: f64,
    pub(super) mean_mrr: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct EvalReport {
    pub(super) timestamp: String,
    pub(super) codebase_path: String,
    pub(super) codegraph_sha: String,
    pub(super) summary: EvalSummary,
    pub(super) results: Vec<EvalResult>,
}

#[derive(Debug, Clone)]
pub(super) struct RunnerExit {
    pub(super) code: i32,
    pub(super) stdout: String,
    pub(super) stderr: String,
    pub(super) report_file: Option<PathBuf>,
}

pub(super) struct OutputCapture {
    pub(super) stdout: String,
    pub(super) stderr: String,
}

impl OutputCapture {
    pub(super) fn new() -> Self {
        Self {
            stdout: String::new(),
            stderr: String::new(),
        }
    }

    pub(super) fn log(&mut self, line: impl AsRef<str>) {
        self.stdout.push_str(line.as_ref());
        self.stdout.push('\n');
    }

    pub(super) fn error(&mut self, line: impl AsRef<str>) {
        self.stderr.push_str(line.as_ref());
        self.stderr.push('\n');
    }
}
