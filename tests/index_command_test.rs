//! Regression coverage for issue #874: `rustcodegraph index` produced 0 nodes / 0
//! edges while `rustcodegraph init -i` worked, and appeared to wipe the graph.
//!
//! Root cause: `index` ran a full extraction against the already-populated DB
//! without clearing it first. Every file's content hash still matched, so the
//! orchestrator skipped re-inserting all of them, and the run reported its delta
//! (after - before = 0) as "0 nodes, 0 edges". The fix makes `index` a true full
//! rebuild: clear, then re-index, so it produces the same complete result as a
//! fresh `init -i`.
//!
//! Exercised end-to-end against the built binary so the CLI wiring (not just the
//! library) is covered.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use rustcodegraph::CodeGraph;

const BIN: &str = env!("CARGO_BIN_EXE_rustcodegraph");
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn run_codegraph(args: &[&str], cwd: &Path) -> String {
    let output = run_codegraph_raw(args, cwd);

    assert!(
        output.status.success(),
        "rustcodegraph {args:?} failed with status {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    String::from_utf8(output.stdout).expect("stdout should be UTF-8")
}

fn run_codegraph_raw(args: &[&str], cwd: &Path) -> Output {
    Command::new(BIN)
        .args(args)
        .current_dir(cwd)
        .env("RUSTCODEGRAPH_NO_DAEMON", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .unwrap_or_else(|err| panic!("failed to run rustcodegraph {args:?}: {err}"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct GraphCounts {
    nodes: u64,
    edges: u64,
}

fn graph_counts(dir: &Path) -> GraphCounts {
    let mut cg = CodeGraph::open_sync(dir).expect("CodeGraph should open");
    let stats = cg.get_stats();
    let counts = GraphCounts {
        nodes: stats.node_count,
        edges: stats.edge_count,
    };
    cg.close();
    counts
}

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(prefix: &str) -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after Unix epoch")
            .as_nanos();
        let suffix = TEMP_COUNTER.fetch_add(1, Ordering::SeqCst);
        let path =
            std::env::temp_dir().join(format!("{prefix}{}-{unique}-{suffix}", std::process::id()));
        fs::create_dir(&path)
            .unwrap_or_else(|err| panic!("failed to create temp dir {}: {err}", path.display()));

        // A couple of files with a call edge so there is a non-trivial graph to
        // (fail to) reproduce.
        fs::write(
            path.join("a.ts"),
            "export function greet(name: string) { return hello(name); }\n\
             export function hello(n: string) { return 'hi ' + n; }\n",
        )
        .expect("a.ts fixture should be written");
        fs::write(
            path.join("b.ts"),
            "import { greet } from './a';\n\
             export function main() { return greet('world'); }\n",
        )
        .expect("b.ts fixture should be written");

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

mod codegraph_index_full_re_index_keeps_the_graph_populated_874 {
    use super::*;

    #[test]
    fn init_without_index_only_initializes_the_database() {
        let temp_dir = TempDir::new("codegraph-index-cmd-");

        let out = run_codegraph(&["init"], temp_dir.path());
        let after_init = graph_counts(temp_dir.path());
        assert!(out.contains("Initialized RustCodeGraph"), "{out}");
        assert!(!out.contains("Indexed "), "{out}");
        assert_eq!(after_init.nodes, 0);
        assert_eq!(after_init.edges, 0);

        run_codegraph(&["init", "-i"], temp_dir.path());
        let after_index = graph_counts(temp_dir.path());
        assert!(after_index.nodes > 0);
        assert!(after_index.edges > 0);
    }

    #[test]
    fn reproduces_inits_node_edge_counts_instead_of_emptying_the_index() {
        let temp_dir = TempDir::new("codegraph-index-cmd-");

        run_codegraph(&["init", "-i"], temp_dir.path());
        let after_init = graph_counts(temp_dir.path());
        assert!(after_init.nodes > 0);
        assert!(after_init.edges > 0);

        let out = run_codegraph(&["index"], temp_dir.path());
        let after_index = graph_counts(temp_dir.path());

        // The graph is still fully populated: `index` rebuilt it, it did not wipe it.
        assert_eq!(after_index.nodes, after_init.nodes);
        assert_eq!(after_index.edges, after_init.edges);

        // ...and the CLI reported the real counts, never the misleading "0 nodes".
        assert!(!out.contains("0 nodes, 0 edges"), "{out}");
        assert!(
            out.contains(&format!("{} nodes", after_init.nodes)),
            "{out}"
        );
    }

    #[test]
    fn is_idempotent_a_second_index_does_not_grow_the_graph() {
        let temp_dir = TempDir::new("codegraph-index-cmd-");

        run_codegraph(&["init", "-i"], temp_dir.path());
        run_codegraph(&["index"], temp_dir.path());
        let first = graph_counts(temp_dir.path());
        run_codegraph(&["index"], temp_dir.path());
        let second = graph_counts(temp_dir.path());

        // A clean rebuild each time: no duplicate (re-resolved) edges accumulating
        // across runs (the C# "+18 edges" symptom in the report).
        assert_eq!(second.nodes, first.nodes);
        assert_eq!(second.edges, first.edges);
    }

    #[test]
    fn quiet_path_also_rebuilds_a_populated_graph() {
        let temp_dir = TempDir::new("codegraph-index-cmd-");

        run_codegraph(&["init", "-i"], temp_dir.path());
        let after_init = graph_counts(temp_dir.path());

        run_codegraph(&["index", "--quiet"], temp_dir.path());
        let after_index = graph_counts(temp_dir.path());

        assert_eq!(after_index.nodes, after_init.nodes);
        assert_eq!(after_index.edges, after_init.edges);
    }

    #[test]
    fn sync_command_refreshes_a_new_file_through_the_incremental_sync_path() {
        let temp_dir = TempDir::new("codegraph-index-cmd-");
        run_codegraph(&["init", "-i"], temp_dir.path());

        fs::write(
            temp_dir.path().join("new_file.rs"),
            "pub fn sync_command_new_symbol() -> usize { 7 }\n",
        )
        .expect("new Rust fixture should be written");

        let out = run_codegraph(&["sync"], temp_dir.path());
        assert!(out.contains("Synced 1 changed file(s)"), "{out}");
        assert!(out.contains("1 added"), "{out}");

        let mut cg = CodeGraph::open_sync(temp_dir.path()).expect("CodeGraph should open");
        let nodes = cg.search_nodes("sync_command_new_symbol", None);
        cg.close();
        assert!(
            nodes
                .iter()
                .any(|result| result.node.name == "sync_command_new_symbol"),
            "new symbol should be searchable after CLI sync"
        );
    }

    #[test]
    fn explicit_uninitialized_subdir_path_is_not_promoted_to_the_parent_project() {
        let temp_dir = TempDir::new("codegraph-index-cmd-");
        fs::create_dir(temp_dir.path().join("child"))
            .expect("child fixture directory should be created");
        fs::write(
            temp_dir.path().join("child").join("child.ts"),
            "export function childOnly() { return 1; }\n",
        )
        .expect("child fixture should be written");

        run_codegraph(&["init", "-i"], temp_dir.path());

        let output = run_codegraph_raw(&["index", "child"], temp_dir.path());

        assert!(
            !output.status.success(),
            "explicit child path should not rebuild the parent project\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("RustCodeGraph not initialized"), "{stderr}");
        assert!(stderr.contains("child"), "{stderr}");
    }
}
