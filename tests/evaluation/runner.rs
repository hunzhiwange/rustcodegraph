//! Evaluation runner harness.
//!
//! Rust port of `__tests__/evaluation/runner.ts`.
//!
//! The TypeScript source is a runnable script rather than a Vitest suite. This
//! port keeps the script behavior in testable functions: EVAL_CODEBASE/argv
//! resolution, `.rustcodegraph/rustcodegraph.db` validation, `git rev-parse --short
//! HEAD`, result scoring, colored table output, JSON report writing, and exit
//! status calculation. The original evaluation case list is preserved here so
//! the runner can be exercised against a real indexed codebase once the Rust
//! context facade reaches TypeScript parity.

#[path = "runner/cases.rs"]
mod cases;
#[path = "runner/execution.rs"]
mod execution;
#[cfg(test)]
#[path = "runner/fixtures.rs"]
mod fixtures;
#[path = "runner/reporting.rs"]
mod reporting;
#[path = "runner/scoring.rs"]
mod scoring;
#[path = "runner/types.rs"]
mod types;

use cases::original_eval_test_cases;
use execution::run_from_inputs;

#[cfg(test)]
use fixtures::{TempDir, cleanup_report, init_indexed_project, init_original_eval_fixture};
#[cfg(test)]
use types::{EvalApi, EvalTestCase};

#[cfg(test)]
mod evaluation_runner {
    use super::*;
    use rustcodegraph::types::NodeKind;
    use std::fs;

    #[test]
    fn reports_usage_and_exits_when_eval_codebase_and_argv_are_missing() {
        let exit = run_from_inputs(None, None, &original_eval_test_cases());

        assert_eq!(exit.code, 1);
        assert!(exit.stdout.is_empty());
        assert!(exit.stderr.contains(
            "Usage: EVAL_CODEBASE=/path/to/codebase npx tsx __tests__/evaluation/runner.ts"
        ));
        assert!(
            exit.stderr
                .contains("or: npx tsx __tests__/evaluation/runner.ts /path/to/codebase")
        );
        assert!(exit.report_file.is_none());
    }

    #[test]
    fn reports_missing_database_for_the_resolved_codebase_path() {
        let temp = TempDir::new("cg-eval-missing-db-");
        let exit = run_from_inputs(None, Some(&temp.path().to_string_lossy()), &[]);

        assert_eq!(exit.code, 1);
        assert!(exit.stdout.is_empty());
        assert!(exit.stderr.contains(&format!(
            "No .rustcodegraph/rustcodegraph.db found at {}",
            temp.path().display()
        )));
        assert!(exit.report_file.is_none());
    }

    #[test]
    fn prefers_eval_codebase_env_over_the_argv_codebase() {
        let env_project = TempDir::new("cg-eval-env-");
        env_project.write(
            "src/env.ts",
            "export function EnvSelectedSymbol(): number { return 1; }\n",
        );
        init_indexed_project(&env_project);

        let argv_project = TempDir::new("cg-eval-argv-");
        argv_project.write(
            "src/argv.ts",
            "export function ArgSelectedSymbol(): number { return 1; }\n",
        );
        init_indexed_project(&argv_project);

        let cases = vec![EvalTestCase {
            id: "search-env-precedence",
            query: "EnvSelectedSymbol",
            api: EvalApi::SearchNodes,
            expected_symbols: &["EnvSelectedSymbol"],
            kinds: Some(vec![NodeKind::Function]),
            options: None,
        }];
        let exit = run_from_inputs(
            Some(&env_project.path().to_string_lossy()),
            Some(&argv_project.path().to_string_lossy()),
            &cases,
        );

        cleanup_report(exit.report_file.as_deref());
        assert_eq!(exit.code, 0, "{}", exit.stdout);
        assert!(
            exit.stdout
                .contains(&format!("Codebase: {}", env_project.path().display()))
        );
        assert!(exit.stdout.contains("SUMMARY: 1/1 passed"));
    }

    #[test]
    fn runs_search_cases_prints_summary_and_writes_a_json_report() {
        let temp = TempDir::new("cg-eval-run-");
        temp.write(
            "src/service.ts",
            "export function TransportService(): number { return sendRequest(); }\n\
             export function sendRequest(): number { return 42; }\n",
        );
        init_indexed_project(&temp);

        let cases = vec![EvalTestCase {
            id: "search-class-exact",
            query: "TransportService",
            api: EvalApi::SearchNodes,
            expected_symbols: &["TransportService"],
            kinds: Some(vec![NodeKind::Function]),
            options: None,
        }];
        let exit = run_from_inputs(None, Some(&temp.path().to_string_lossy()), &cases);
        let report_file = exit
            .report_file
            .clone()
            .expect("successful runner should write a report file");
        let report_json =
            fs::read_to_string(&report_file).expect("eval report should be readable JSON");
        cleanup_report(Some(&report_file));

        assert_eq!(exit.code, 0, "{}", exit.stdout);
        assert!(exit.stdout.contains("CodeGraph Eval "));
        assert!(exit.stdout.contains("Cases:    1"));
        assert!(exit.stdout.contains("PASS"));
        assert!(exit.stdout.contains("recall=1.00"));
        assert!(exit.stdout.contains("mrr=1.00"));
        assert!(
            exit.stdout
                .contains("SUMMARY: 1/1 passed | recall=1.00 | mrr=1.00")
        );
        assert!(exit.stdout.contains("Report saved:"));

        let report: serde_json::Value =
            serde_json::from_str(&report_json).expect("eval report should parse");
        assert_eq!(
            report["codebasePath"].as_str(),
            Some(temp.path().to_string_lossy().as_ref())
        );
        assert_eq!(report["summary"]["total"], 1);
        assert_eq!(report["summary"]["passed"], 1);
        assert_eq!(report["summary"]["failed"], 0);
        assert_eq!(report["results"][0]["caseId"], "search-class-exact");
        assert_eq!(report["results"][0]["foundSymbols"][0], "TransportService");
    }

    #[test]
    fn preserves_the_original_typescript_eval_case_list() {
        let cases = original_eval_test_cases();
        let ids = cases.iter().map(|case| case.id).collect::<Vec<_>>();

        assert_eq!(
            ids,
            vec![
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
            ]
        );
        assert_eq!(
            cases
                .iter()
                .filter(|case| case.api == EvalApi::SearchNodes)
                .count(),
            6
        );
        assert_eq!(
            cases
                .iter()
                .filter(|case| case.api == EvalApi::FindRelevantContext)
                .count(),
            6
        );
        assert_eq!(
            cases[6].expected_symbols,
            &[
                "RestController",
                "RestHandler",
                "BaseRestHandler",
                "RestRequest"
            ]
        );
        assert_eq!(
            cases[6].options.expect("options should exist").search_limit,
            Some(8)
        );
        assert_eq!(
            cases[6]
                .options
                .expect("options should exist")
                .traversal_depth,
            Some(3)
        );
        assert_eq!(
            cases[6].options.expect("options should exist").max_nodes,
            Some(80)
        );
        assert_eq!(
            cases[6].options.expect("options should exist").min_score,
            Some(0.2)
        );
    }

    #[test]
    fn runs_the_original_evaluation_cases_against_an_indexed_codebase() {
        let codebase = init_original_eval_fixture();
        let exit = run_from_inputs(
            Some(&codebase.path().to_string_lossy()),
            None,
            &original_eval_test_cases(),
        );
        let report_json = exit
            .report_file
            .as_ref()
            .map(|path| fs::read_to_string(path).expect("eval report should be readable JSON"));
        cleanup_report(exit.report_file.as_deref());

        assert_eq!(
            exit.code, 0,
            "stdout:\n{}\nstderr:\n{}",
            exit.stdout, exit.stderr
        );
        assert!(exit.stdout.contains("SUMMARY: 12/12 passed"));

        let report: serde_json::Value = serde_json::from_str(
            report_json
                .as_deref()
                .expect("successful original eval run should write a report"),
        )
        .expect("eval report should parse");
        assert_eq!(report["summary"]["total"], 12);
        assert_eq!(report["summary"]["passed"], 12);
        assert_eq!(report["summary"]["failed"], 0);
    }
}
