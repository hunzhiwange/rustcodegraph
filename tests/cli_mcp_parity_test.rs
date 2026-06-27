//! CLI query commands should reuse the MCP tool output path instead of drifting
//! into a second, weaker implementation.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use rustcodegraph::{CodeGraph, IndexOptions};

const BIN: &str = env!("CARGO_BIN_EXE_rustcodegraph");
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

struct Fixture {
    path: PathBuf,
    cg: CodeGraph,
}

impl Fixture {
    fn new() -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after Unix epoch")
            .as_nanos();
        let suffix = TEMP_COUNTER.fetch_add(1, Ordering::SeqCst);
        let path = std::env::temp_dir().join(format!(
            "rustcodegraph-cli-mcp-parity-{}-{unique}-{suffix}",
            std::process::id()
        ));
        fs::create_dir(&path)
            .unwrap_or_else(|err| panic!("failed to create temp dir {}: {err}", path.display()));
        fs::write(
            path.join("auth.ts"),
            "export function login(req: Request) {\n  return sessionMiddleware(req);\n}\n\n\
             export function sessionMiddleware(req: Request) {\n  return req.session.user;\n}\n",
        )
        .expect("auth fixture should be written");

        let mut cg = CodeGraph::init_sync(&path).expect("CodeGraph should initialize");
        let result = cg.index_all(IndexOptions::default());
        assert!(
            result.success,
            "index_all should succeed, errors: {:?}",
            result.errors
        );
        Self { path, cg }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.cg.close();
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn run_cli(fixture: &Fixture, args: &[&str]) -> String {
    let output = Command::new(BIN)
        .args(args)
        .arg("-p")
        .arg(fixture.path())
        .env("RUSTCODEGRAPH_NO_DAEMON", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .unwrap_or_else(|err| panic!("failed to run rustcodegraph {args:?}: {err}"));

    assert!(
        output.status.success(),
        "rustcodegraph {args:?} failed with status {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    String::from_utf8(output.stdout).expect("stdout should be UTF-8")
}

#[test]
fn explore_cli_uses_mcp_flow_output_for_sentence_queries() {
    let fixture = Fixture::new();
    let out = run_cli(
        &fixture,
        &["explore", "how does login reach sessionMiddleware?"],
    );

    assert!(out.contains("## Flow"), "{out}");
    assert!(
        out.contains("login (auth.ts:1) --calls--> sessionMiddleware (auth.ts:5)"),
        "{out}"
    );
    assert!(out.contains("### Source Code"), "{out}");
}

#[test]
fn graph_query_cli_uses_mcp_sectioned_output() {
    let fixture = Fixture::new();
    let out = run_cli(&fixture, &["callers", "sessionMiddleware"]);

    assert!(out.contains("## Callers for `sessionMiddleware`"), "{out}");
    assert!(
        out.contains("function `login` at auth.ts:1 via calls"),
        "{out}"
    );
}

#[test]
fn node_file_cli_uses_mcp_file_view_output() {
    let fixture = Fixture::new();
    let out = run_cli(&fixture, &["node", "--file", "auth.ts", "--limit", "3"]);

    assert!(out.contains("## auth.ts"), "{out}");
    assert!(out.contains("used by 0 files"), "{out}");
    assert!(out.contains("Showing lines 1-3 of"), "{out}");
    assert!(out.contains("1\texport function login"), "{out}");
}

#[test]
fn cli_query_commands_are_not_disabled_by_the_mcp_allowlist() {
    let fixture = Fixture::new();
    let output = Command::new(BIN)
        .args(["explore", "login sessionMiddleware", "-p"])
        .arg(fixture.path())
        .env("RUSTCODEGRAPH_NO_DAEMON", "1")
        .env("RUSTCODEGRAPH_MCP_TOOLS", "search")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("rustcodegraph should run");

    assert!(
        output.status.success(),
        "rustcodegraph explore failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("stdout should be UTF-8");
    assert!(!stdout.contains("disabled via RUSTCODEGRAPH_MCP_TOOLS"));
    assert!(stdout.contains("## Flow"), "{stdout}");
}
