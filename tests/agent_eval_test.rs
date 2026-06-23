use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rustcodegraph::agent_eval::{
    parse_arms_report, parse_bench_readme_report, parse_run_report, parse_session_report_with_home,
    probe_explore_text, probe_node_text, probe_sweep_report, seq_matrix_report,
};
use rustcodegraph::{CodeGraph, IndexOptions};

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(prefix: &str) -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after Unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("{prefix}-{}-{unique}", std::process::id()));
        fs::create_dir(&path)
            .unwrap_or_else(|err| panic!("failed to create temp dir {}: {err}", path.display()));
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn write_jsonl(path: &Path, lines: &[&str]) {
    fs::write(path, format!("{}\n", lines.join("\n")))
        .unwrap_or_else(|err| panic!("failed to write {}: {err}", path.display()));
}

#[test]
fn parse_run_reports_tool_counts_exposure_and_result_usage() {
    let temp = TempDir::new("codegraph-agent-eval-run");
    let log = temp.path().join("run.jsonl");
    write_jsonl(
        &log,
        &[
            r#"{"type":"system","subtype":"init","tools":["mcp__rustcodegraph__rustcodegraph_explore","Read"]}"#,
            r#"{"type":"assistant","message":{"usage":{"input_tokens":10,"output_tokens":5,"cache_read_input_tokens":3,"cache_creation_input_tokens":2},"content":[{"type":"tool_use","id":"t1","name":"Read","input":{"file_path":"/tmp/src/lib.rs"}},{"type":"tool_use","id":"t2","name":"mcp__rustcodegraph__rustcodegraph_explore","input":{"query":"alpha beta"}}]}}"#,
            r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t2","content":[{"text":"hello"}]}]}}"#,
            r#"{"type":"result","subtype":"success","duration_ms":12000,"num_turns":3,"total_cost_usd":0.1234,"usage":{"input_tokens":100,"cache_read_input_tokens":20,"cache_creation_input_tokens":5,"output_tokens":9}} "#,
        ],
    );

    let report = parse_run_report(&log).expect("parse-run report should render");

    assert!(
        report.contains("rustcodegraph tools exposed: 1"),
        "{report}"
    );
    assert!(report.contains("Tool calls (2):"), "{report}");
    assert!(
        report.contains(r#"by type: {"Read":1,"mcp__rustcodegraph__rustcodegraph_explore":1}"#),
        "{report}"
    );
    assert!(report.contains("1. Read lib.rs"), "{report}");
    assert!(
        report.contains(r#"2. mcp__rustcodegraph__rustcodegraph_explore "alpha beta""#),
        "{report}"
    );
    assert!(
        report.contains("Result: success | duration 12s | turns 3"),
        "{report}"
    );
    assert!(
        report.contains("tokens: in=125 out=9 | cost $0.123"),
        "{report}"
    );
}

#[test]
fn parse_session_report_aggregates_latest_session_and_subagents() {
    let temp = TempDir::new("codegraph-agent-eval-session");
    let home = temp.path().join("home");
    let project = temp.path().join("workspace/repo");
    fs::create_dir_all(&project).expect("project fixture should be created");

    let real_project = fs::canonicalize(&project).expect("project path should canonicalize");
    let escaped_project = real_project.to_string_lossy().replace('/', "-");
    let project_log_dir = home.join(".claude/projects").join(escaped_project);
    fs::create_dir_all(&project_log_dir).expect("claude project log dir should be created");

    write_jsonl(
        &project_log_dir.join("older-session.jsonl"),
        &[
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"old","name":"Read","input":{"file_path":"/tmp/old.rs"}}]}}"#,
        ],
    );
    std::thread::sleep(Duration::from_millis(20));

    write_jsonl(
        &project_log_dir.join("newer-session.jsonl"),
        &[
            r#"{"type":"assistant","message":{"usage":{"input_tokens":10000,"output_tokens":2000,"cache_read_input_tokens":3000,"cache_creation_input_tokens":500},"content":[{"type":"tool_use","id":"m1","name":"mcp__rustcodegraph__rustcodegraph_explore","input":{"query":"alpha beta"}},{"type":"tool_use","id":"m2","name":"Read","input":{"file_path":"/tmp/src/main.rs"}},{"type":"tool_use","id":"m3","name":"Bash","input":{"command":"rg alpha"}}]}}"#,
        ],
    );

    let sub_dir = project_log_dir.join("newer-session/subagents");
    fs::create_dir_all(&sub_dir).expect("subagent dir should be created");
    write_jsonl(
        &sub_dir.join("explore-1.jsonl"),
        &[
            r#"{"type":"assistant","message":{"usage":{"input_tokens":5000,"output_tokens":1000,"cache_read_input_tokens":800,"cache_creation_input_tokens":200},"content":[{"type":"tool_use","id":"s1","name":"Read","input":{"file_path":"/tmp/src/lib.rs"}},{"type":"tool_use","id":"s2","name":"Grep","input":{"pattern":"beta"}},{"type":"tool_use","id":"s3","name":"mcp__rustcodegraph__rustcodegraph_explore","input":{"query":"beta gamma"}}]}}"#,
        ],
    );
    write_jsonl(
        &sub_dir.join("explore-2.jsonl"),
        &[
            r#"{"type":"assistant","message":{"usage":{"input_tokens":2500,"output_tokens":500,"cache_read_input_tokens":400,"cache_creation_input_tokens":100},"content":[{"type":"tool_use","id":"s4","name":"Read","input":{"file_path":"/tmp/src/mod.rs"}},{"type":"tool_use","id":"s5","name":"mcp__rustcodegraph__rustcodegraph_explore","input":{"query":"gamma delta"}}]}}"#,
        ],
    );

    let report = parse_session_report_with_home(&project, &home)
        .expect("parse-session report should render");

    assert!(report.contains("session: newer-session"), "{report}");
    assert!(report.contains("MAIN thread tools:"), "{report}");
    assert!(
        report.contains("mcp__rustcodegraph__rustcodegraph_explore"),
        "{report}"
    );
    assert!(
        report.contains("SUBAGENT tools (2 subagent transcripts):"),
        "{report}"
    );
    assert!(report.contains("      2  Read"), "{report}");
    assert!(
        report.contains("VERDICT: rustcodegraph_explore used 3x | Read 3 | Grep/Bash 2"),
        "{report}"
    );
    assert!(
        report.contains("TOKENS: gen 3.5k | fresh-in 18.3k | cached-in 4.2k | billable≈ 21.8k"),
        "{report}"
    );
}

#[test]
fn parse_bench_readme_sums_per_turn_assistant_tokens() {
    let temp = TempDir::new("codegraph-agent-eval-bench");
    let run_dir = temp.path().join("vscode/run1");
    fs::create_dir_all(&run_dir).expect("fixture dirs should be created");
    write_jsonl(
        &run_dir.join("run-headless-with.jsonl"),
        &[
            r#"{"type":"assistant","message":{"usage":{"input_tokens":300,"output_tokens":100,"cache_read_input_tokens":400,"cache_creation_input_tokens":200},"content":[{"type":"tool_use","id":"t1","name":"mcp__rustcodegraph__rustcodegraph_explore","input":{"query":"alpha"}}]}}"#,
            r#"{"type":"result","subtype":"success","duration_ms":10000,"num_turns":2,"total_cost_usd":0.1}"#,
        ],
    );
    write_jsonl(
        &run_dir.join("run-headless-without.jsonl"),
        &[
            r#"{"type":"assistant","message":{"usage":{"input_tokens":900,"output_tokens":100,"cache_read_input_tokens":800,"cache_creation_input_tokens":200},"content":[{"type":"tool_use","id":"t1","name":"Read","input":{"file_path":"/tmp/a.rs"}}]}}"#,
            r#"{"type":"result","subtype":"success","duration_ms":20000,"num_turns":4,"total_cost_usd":0.2}"#,
        ],
    );

    let report = parse_bench_readme_report(temp.path()).expect("bench-readme report should render");

    assert!(report.contains("vscode"), "{report}");
    assert!(report.contains("1/1"), "{report}");
    assert!(report.contains("10s->20s"), "{report}");
    assert!(report.contains("1k->2k (50%)"), "{report}");
    assert!(
        report.contains("AVERAGE saved:  cost 50%  ·  tokens 50%  ·  time 50%  ·  tool calls 0%"),
        "{report}"
    );
}

#[test]
fn parse_arms_report_summarizes_arm_ablation_logs() {
    let temp = TempDir::new("codegraph-agent-eval-arms");
    let repo_dir = temp.path().join("demo-repo");
    fs::create_dir_all(&repo_dir).expect("fixture dirs should be created");
    write_jsonl(
        &repo_dir.join("A-r1.jsonl"),
        &[
            r#"{"type":"system","subtype":"init","tools":["mcp__rustcodegraph__rustcodegraph_trace","mcp__rustcodegraph__rustcodegraph_explore"]}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"r1","name":"Read","input":{"file_path":"/tmp/src/lib.rs"}},{"type":"tool_use","id":"t1","name":"mcp__rustcodegraph__rustcodegraph_trace","input":{"query":"alpha beta"}},{"type":"tool_use","id":"g1","name":"Grep","input":{"pattern":"alpha"}}]}}"#,
            r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":[{"text":"trace body"}]}]}}"#,
            r#"{"type":"result","subtype":"success","duration_ms":10000,"num_turns":3,"total_cost_usd":0.2}"#,
        ],
    );
    write_jsonl(
        &repo_dir.join("A-r2.jsonl"),
        &[
            r#"{"type":"system","subtype":"init","tools":["mcp__rustcodegraph__rustcodegraph_trace","mcp__rustcodegraph__rustcodegraph_explore"]}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"r2","name":"Read","input":{"file_path":"/tmp/src/main.rs"}},{"type":"tool_use","id":"g2","name":"Glob","input":{"pattern":"*.rs"}}]}}"#,
            r#"{"type":"result","subtype":"success","duration_ms":20000,"num_turns":5,"total_cost_usd":0.3}"#,
        ],
    );
    write_jsonl(
        &repo_dir.join("I-r1.jsonl"),
        &[
            r#"{"type":"system","subtype":"init","tools":["mcp__rustcodegraph__rustcodegraph_trace","mcp__rustcodegraph__rustcodegraph_explore"]}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t2","name":"mcp__rustcodegraph__rustcodegraph_trace","input":{"query":"gamma delta"}}]}}"#,
            r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t2","content":[{"text":"complete flow"}]}]}}"#,
            r#"{"type":"result","subtype":"success","duration_ms":30000,"num_turns":6,"total_cost_usd":0.4}"#,
        ],
    );

    let report = parse_arms_report(temp.path()).expect("parse-arms report should render");
    let compact = report.split_whitespace().collect::<Vec<_>>().join(" ");

    assert!(
        report.contains("=== PER REPO x ARM (avg over runs) ==="),
        "{report}"
    );
    assert!(
        compact.contains("demo-repo A all/none(old) 2 1/2 1.0 0.0 4.0 15s"),
        "{report}"
    );
    assert!(
        compact.contains("A all/none(old) 1/2 1.00 1.00 0.0 4.0 15s $0.250 (1 repos)"),
        "{report}"
    );
    assert!(
        compact.contains("I bodytrace+dest 1/1 0.00 0.00 0.0 6.0 30s $0.400 (1 repos)"),
        "{report}"
    );
}

#[test]
fn seq_matrix_report_summarizes_saved_matrix_logs() {
    let temp = TempDir::new("codegraph-agent-eval-seq-matrix");
    let root = temp.path().join("ab-matrix");
    let docs_root = temp.path().join("docs-root");
    fs::create_dir_all(docs_root.join("docs/benchmarks")).expect("docs fixture should be created");
    fs::write(
        docs_root.join("docs/benchmarks/rustcodegraph-ab-matrix.md"),
        [
            "| language | size | repo | files |",
            "|---|---|---|---:|",
            "| Rust | S | `demo-small` | 10 |",
            "| TypeScript | M | `demo-medium` | 500 |",
        ]
        .join("\n"),
    )
    .expect("repo metadata fixture should be written");

    let small = root.join("cell-small");
    let medium = root.join("cell-medium");
    fs::create_dir_all(&small).expect("small cell should be created");
    fs::create_dir_all(&medium).expect("medium cell should be created");
    fs::write(
        root.join("cell-small.log"),
        "repo: https://example.test/demo-small\n",
    )
    .expect("small cell log should be written");
    fs::write(
        root.join("cell-medium.log"),
        "repo: https://example.test/demo-medium\n",
    )
    .expect("medium cell log should be written");

    write_jsonl(
        &small.join("run-headless-with.jsonl"),
        &[
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"ts1","name":"ToolSearch","input":{"query":"rustcodegraph"}},{"type":"tool_use","id":"cx1","name":"mcp__rustcodegraph__rustcodegraph_context","input":{"query":"alpha"}},{"type":"tool_use","id":"ex1","name":"mcp__rustcodegraph__rustcodegraph_explore","input":{"query":"alpha beta"}}]}}"#,
            &format!(
                r#"{{"type":"user","message":{{"content":[{{"type":"tool_result","tool_use_id":"cx1","content":[{{"text":"{}"}}]}},{{"type":"tool_result","tool_use_id":"ex1","content":[{{"text":"{}"}}]}}]}}}}"#,
                "c".repeat(1500),
                "e".repeat(2500)
            ),
            r#"{"type":"result","subtype":"success","duration_ms":10000,"num_turns":4,"total_cost_usd":0.10}"#,
        ],
    );
    write_jsonl(
        &small.join("run-headless-without.jsonl"),
        &[
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"r1","name":"Read","input":{"file_path":"/tmp/a.rs"}}]}}"#,
            r#"{"type":"result","subtype":"success","duration_ms":20000,"num_turns":6,"total_cost_usd":0.20}"#,
        ],
    );
    write_jsonl(
        &medium.join("run-headless-with.jsonl"),
        &[
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"ts2","name":"ToolSearch","input":{"query":"rustcodegraph"}},{"type":"tool_use","id":"tr1","name":"mcp__rustcodegraph__rustcodegraph_trace","input":{"query":"gamma delta"}},{"type":"tool_use","id":"nd1","name":"mcp__rustcodegraph__rustcodegraph_node","input":{"symbol":"delta"}}]}}"#,
            &format!(
                r#"{{"type":"user","message":{{"content":[{{"type":"tool_result","tool_use_id":"tr1","content":[{{"text":"{}"}}]}},{{"type":"tool_result","tool_use_id":"nd1","content":[{{"text":"{}"}}]}}]}}}}"#,
                "t".repeat(800),
                "n".repeat(1200)
            ),
            r#"{"type":"result","subtype":"success","duration_ms":30000,"num_turns":5,"total_cost_usd":0.30}"#,
        ],
    );
    write_jsonl(
        &medium.join("run-headless-without.jsonl"),
        &[
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"g1","name":"Grep","input":{"pattern":"gamma"}}]}}"#,
            r#"{"type":"result","subtype":"success","duration_ms":40000,"num_turns":8,"total_cost_usd":0.40}"#,
        ],
    );

    let report = seq_matrix_report(&root, &docs_root).expect("seq-matrix report should render");
    let compact = report.split_whitespace().collect::<Vec<_>>().join(" ");

    assert!(
        report.contains("=== PER-CELL: with-arm codegraph sequence + payload"),
        "{report}"
    );
    assert!(
        compact.contains("demo-small 10 no context,explore 4.0 4/6"),
        "{report}"
    );
    assert!(
        compact.contains("demo-medium 500 YES trace,node 2.0 5/8"),
        "{report}"
    );
    assert!(report.contains("trace called in 1/2 cells"), "{report}");
    assert!(
        report.contains("after-trace follow-ups: demo-medium[node]"),
        "{report}"
    );
    assert!(
        compact.contains("S(<200) n=1 avg cgOut=4.0K range 4.0-4.0K"),
        "{report}"
    );
    assert!(
        compact.contains("explore calls=1 avg=2.5K/call total=2.5K"),
        "{report}"
    );
    assert!(
        report.contains("turns: with=9  without=14  (36% fewer with)"),
        "{report}"
    );
    assert!(
        report.contains("every with-arm opens with a ToolSearch round-trip (deferred tools): YES"),
        "{report}"
    );
}

#[test]
fn probe_sweep_reports_filtered_empty_sweep_and_rejects_retired_tools() {
    let report = probe_sweep_report(&["--repos=not-in-corpus".to_string()])
        .expect("filtered probe sweep should render without corpus repos");

    assert!(
        report.contains("=== probe-sweep tool=explore n=0"),
        "{report}"
    );
    assert!(
        report.contains("median=0c  total=0c  manifest=0/0  top-handler=0/0"),
        "{report}"
    );

    let err = probe_sweep_report(&["--tool=context".to_string()])
        .expect_err("retired probe tools should be rejected");

    assert!(
        err.contains("context and trace are retired"),
        "unexpected error: {err}"
    );
}

#[test]
fn probe_explore_uses_the_rust_mcp_tool_handler() {
    let temp = TempDir::new("codegraph-agent-eval-probe");
    fs::write(
        temp.path().join("lib.rs"),
        "pub fn alpha() {\n    beta();\n}\n\npub fn beta() {}\n",
    )
    .expect("fixture should be written");
    let mut cg = CodeGraph::init_sync(temp.path()).expect("CodeGraph should initialize");
    let index = cg.index_all(IndexOptions::default());
    assert!(index.success, "indexing should succeed: {index:?}");
    cg.close();

    let text = probe_explore_text(temp.path(), "alpha beta")
        .expect("probe-explore should return MCP text");

    assert!(text.contains("## Explore Results"), "{text}");
    assert!(text.contains("alpha"), "{text}");
    assert!(text.contains("beta"), "{text}");
}

#[test]
fn probe_node_preserves_include_code_for_ambiguous_symbols() {
    let temp = TempDir::new("codegraph-agent-eval-probe-node");
    fs::write(
        temp.path().join("first.rs"),
        "pub fn task() {\n    alpha_dep();\n}\n\nfn alpha_dep() {}\n",
    )
    .expect("first fixture should be written");
    fs::write(
        temp.path().join("second.rs"),
        "pub fn task() {\n    beta_dep();\n}\n\nfn beta_dep() {}\n",
    )
    .expect("second fixture should be written");
    let mut cg = CodeGraph::init_sync(temp.path()).expect("CodeGraph should initialize");
    let index = cg.index_all(IndexOptions::default());
    assert!(index.success, "indexing should succeed: {index:?}");
    cg.close();

    let text =
        probe_node_text(temp.path(), "task", true).expect("probe-node should return MCP text");

    assert!(
        text.contains("Found 2 definitions named \"task\""),
        "{text}"
    );
    assert!(text.contains("first.rs"), "{text}");
    assert!(text.contains("second.rs"), "{text}");
    assert!(text.contains("### Source"), "{text}");
    assert!(text.contains("alpha_dep();"), "{text}");
    assert!(text.contains("beta_dep();"), "{text}");
    assert!(text.contains("### Trail - rustcodegraph_node"), "{text}");
    assert!(text.contains("alpha_dep"), "{text}");
    assert!(text.contains("beta_dep"), "{text}");
}
