use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Instant, SystemTime};

use rustcodegraph::CodeGraph;
use rustcodegraph::types::{FindRelevantContextOptions, SearchOptions};

use super::reporting::{iso_timestamp, write_report};
use super::scoring::{score_find_relevant_context, score_search_nodes};
use super::types::{EvalApi, EvalReport, EvalSummary, EvalTestCase, OutputCapture, RunnerExit};

pub(super) fn run_from_inputs(
    env_codebase: Option<&str>,
    argv_codebase: Option<&str>,
    cases: &[EvalTestCase],
) -> RunnerExit {
    let mut out = OutputCapture::new();
    let Some(codebase_path) = env_codebase.or(argv_codebase) else {
        out.error("Usage: EVAL_CODEBASE=/path/to/codebase npx tsx __tests__/evaluation/runner.ts");
        out.error("   or: npx tsx __tests__/evaluation/runner.ts /path/to/codebase");
        return RunnerExit {
            code: 1,
            stdout: out.stdout,
            stderr: out.stderr,
            report_file: None,
        };
    };

    let resolved_path = resolve_path(codebase_path);
    if !resolved_path
        .join(".rustcodegraph")
        .join("rustcodegraph.db")
        .exists()
    {
        out.error(format!(
            "No .rustcodegraph/rustcodegraph.db found at {}",
            resolved_path.display()
        ));
        return RunnerExit {
            code: 1,
            stdout: out.stdout,
            stderr: out.stderr,
            report_file: None,
        };
    }

    let codegraph_sha = codegraph_sha();
    out.log(format!(
        "\nCodeGraph Eval \u{2014} {}",
        resolved_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("")
    ));
    out.log(format!("Codebase: {}", resolved_path.display()));
    out.log(format!("Commit:   {codegraph_sha}"));
    out.log(format!("Cases:    {}", cases.len()));
    out.log("");

    match run_cases(&resolved_path, &codegraph_sha, cases, &mut out) {
        Ok((code, report_file)) => RunnerExit {
            code,
            stdout: out.stdout,
            stderr: out.stderr,
            report_file: Some(report_file),
        },
        Err(err) => {
            out.error(err);
            RunnerExit {
                code: 1,
                stdout: out.stdout,
                stderr: out.stderr,
                report_file: None,
            }
        }
    }
}

fn run_cases(
    resolved_path: &Path,
    codegraph_sha: &str,
    cases: &[EvalTestCase],
    out: &mut OutputCapture,
) -> Result<(i32, PathBuf), String> {
    let mut cg = CodeGraph::open_sync(resolved_path).map_err(|err| err.to_string())?;
    let mut results = Vec::new();

    for tc in cases {
        let start = Instant::now();

        match tc.api {
            EvalApi::SearchNodes => {
                let search_results = cg.search_nodes(tc.query, Some(search_options(tc)));
                let latency = start.elapsed().as_secs_f64() * 1000.0;
                let result =
                    score_search_nodes(tc.id, tc.expected_symbols, &search_results, latency);
                results.push(result);
            }
            EvalApi::FindRelevantContext => {
                let subgraph = cg.find_relevant_context(tc.query, Some(find_options(tc)));
                let latency = start.elapsed().as_secs_f64() * 1000.0;
                let result =
                    score_find_relevant_context(tc.id, tc.expected_symbols, &subgraph, latency);
                results.push(result);
            }
        }
    }

    cg.close();

    let max_id_len = results
        .iter()
        .map(|result| result.case_id.len())
        .max()
        .unwrap_or(0);

    for result in &results {
        let status = if result.pass {
            "\x1b[32mPASS\x1b[0m"
        } else {
            "\x1b[31mFAIL\x1b[0m"
        };
        let id = format!("{:<width$}", result.case_id, width = max_id_len);
        let recall = format!("recall={:.2}", result.recall);
        let extra = result
            .edge_density
            .map(|density| format!("density={density:.2}"))
            .unwrap_or_else(|| format!("mrr={:.2}", result.mrr));
        let latency = format!("{}ms", result.latency_ms.round() as u64);

        out.log(format!("  {id}  {status}  {recall}  {extra}  {latency}"));

        if !result.missed_symbols.is_empty() {
            out.log(format!(
                "  {}        missed: {}",
                " ".repeat(max_id_len),
                result.missed_symbols.join(", ")
            ));
        }
    }

    let passed = results.iter().filter(|result| result.pass).count();
    let failed = results.len() - passed;
    let mean_recall = if results.is_empty() {
        0.0
    } else {
        results.iter().map(|result| result.recall).sum::<f64>() / results.len() as f64
    };
    let mrr_results = results
        .iter()
        .filter(|result| result.mrr > 0.0 || result.case_id.starts_with("search-"))
        .collect::<Vec<_>>();
    let mean_mrr = if mrr_results.is_empty() {
        0.0
    } else {
        mrr_results.iter().map(|result| result.mrr).sum::<f64>() / mrr_results.len() as f64
    };

    out.log("");
    let summary_color = if failed == 0 { "\x1b[32m" } else { "\x1b[33m" };
    out.log(format!(
        "{summary_color}SUMMARY: {passed}/{} passed | recall={mean_recall:.2} | mrr={mean_mrr:.2}\x1b[0m",
        results.len()
    ));

    let report = EvalReport {
        timestamp: iso_timestamp(SystemTime::now()),
        codebase_path: resolved_path.to_string_lossy().into_owned(),
        codegraph_sha: codegraph_sha.to_owned(),
        summary: EvalSummary {
            total: results.len(),
            passed,
            failed,
            mean_recall,
            mean_mrr,
        },
        results,
    };

    let report_file = write_report(&report)?;
    out.log(format!("\nReport saved: {}", report_file.display()));

    Ok((if failed > 0 { 1 } else { 0 }, report_file))
}

fn search_options(tc: &EvalTestCase) -> SearchOptions {
    SearchOptions {
        kinds: tc.kinds.clone(),
        languages: None,
        include_patterns: None,
        exclude_patterns: None,
        limit: Some(tc.options.and_then(|options| options.limit).unwrap_or(10)),
        offset: None,
        case_sensitive: None,
    }
}

fn find_options(tc: &EvalTestCase) -> FindRelevantContextOptions {
    let options = tc.options.unwrap_or_default();
    FindRelevantContextOptions {
        search_limit: Some(options.search_limit.unwrap_or(8)),
        traversal_depth: Some(options.traversal_depth.unwrap_or(3)),
        max_nodes: Some(options.max_nodes.unwrap_or(80)),
        min_score: Some(options.min_score.unwrap_or(0.2)),
        edge_kinds: None,
        node_kinds: None,
    }
}

fn resolve_path(path: &str) -> PathBuf {
    let path = Path::new(path);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .expect("current dir should resolve")
            .join(path)
    }
}

fn codegraph_sha() -> String {
    let output = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output();

    match output {
        Ok(output) if output.status.success() => String::from_utf8(output.stdout)
            .map(|stdout| stdout.trim().to_owned())
            .ok()
            .filter(|stdout| !stdout.is_empty())
            .unwrap_or_else(|| "unknown".to_owned()),
        _ => "unknown".to_owned(),
    }
}
