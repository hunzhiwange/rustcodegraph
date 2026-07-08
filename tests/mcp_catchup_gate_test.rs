//! MCP catch-up gate.
//!
//! This is the Rust port of `__tests__/mcp-catchup-gate.test.ts`. The first
//! MCP tool call runs the engine's post-open filesystem reconcile before
//! serving results, so deleted or edited files from an offline interval do not
//! leak out of the existing index.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rustcodegraph::mcp::tools::{ToolHandler, ToolResult};
use rustcodegraph::{CodeGraph, IndexOptions};
use serde_json::{Map, Value, json};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

struct TempProject {
    root: PathBuf,
}

impl TempProject {
    fn new() -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after Unix epoch")
            .as_nanos();
        let suffix = TEMP_COUNTER.fetch_add(1, Ordering::SeqCst);
        let root = std::env::temp_dir().join(format!(
            "codegraph-catchup-gate-{}-{unique}-{suffix}",
            std::process::id()
        ));
        fs::create_dir_all(root.join("src")).unwrap_or_else(|err| {
            panic!(
                "failed to create fixture src directory {}: {err}",
                root.display()
            )
        });
        Self { root }
    }

    fn path(&self) -> &Path {
        &self.root
    }

    fn src(&self, name: &str) -> PathBuf {
        self.root.join("src").join(name)
    }

    fn write_src(&self, name: &str, contents: &str) {
        let path = self.src(name);
        fs::write(&path, contents)
            .unwrap_or_else(|err| panic!("failed to write fixture {}: {err}", path.display()));
    }

    fn remove_src(&self, name: &str) {
        let path = self.src(name);
        fs::remove_file(&path)
            .unwrap_or_else(|err| panic!("failed to remove fixture {}: {err}", path.display()));
    }
}

impl Drop for TempProject {
    fn drop(&mut self) {
        if self.root.exists() {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}

struct Fixture {
    temp: TempProject,
    cg: CodeGraph,
    handler: ToolHandler,
}

impl Fixture {
    fn new() -> Self {
        let temp = TempProject::new();
        temp.write_src("survivor.ts", "export function survivor() { return 1; }\n");
        temp.write_src(
            "deleted-later.ts",
            "export function deletedLater() { return 2; }\n",
        );

        let mut cg = CodeGraph::init_sync(temp.path()).expect("failed to initialize CodeGraph");
        let result = cg.index_all(IndexOptions::default());
        assert!(
            result.success,
            "index_all should succeed, errors: {:?}",
            result.errors
        );
        let mut handler = ToolHandler::new(true);
        handler.set_default_code_graph(&cg);

        Self { temp, cg, handler }
    }

    fn execute_search(&mut self, query: &str) -> ToolResult {
        self.handler
            .execute("rustcodegraph_search", &search_args(query))
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.handler.close_all();
        self.cg.unwatch();
        self.cg.close();
    }
}

fn first_text(result: &ToolResult) -> &str {
    result
        .content
        .first()
        .map(|content| content.text.as_str())
        .unwrap_or("")
}

fn search_args(query: &str) -> Map<String, Value> {
    let mut args = Map::new();
    args.insert("query".to_string(), json!(query));
    args
}

mod mcp_catch_up_gate {
    use super::*;

    #[test]
    fn awaits_the_gate_before_serving_the_first_tool_call() {
        let mut fixture = Fixture::new();
        let gate_resolved = Arc::new(AtomicBool::new(false));
        let gate_resolved_in_gate = Arc::clone(&gate_resolved);
        fixture.handler.set_catch_up_gate(move || {
            thread::sleep(Duration::from_millis(80));
            gate_resolved_in_gate.store(true, Ordering::SeqCst);
            Ok(())
        });

        let res = fixture.execute_search("survivor");

        assert!(gate_resolved.load(Ordering::SeqCst));
        assert_ne!(res.is_error, Some(true));
        assert!(first_text(&res).contains("survivor"));
    }

    #[test]
    fn status_is_diagnostic_and_does_not_trigger_catch_up() {
        let mut fixture = Fixture::new();
        let await_count = Arc::new(AtomicUsize::new(0));
        let await_count_in_gate = Arc::clone(&await_count);
        fixture.handler.set_catch_up_gate(move || {
            await_count_in_gate.fetch_add(1, Ordering::SeqCst);
            Ok(())
        });

        let res = fixture.handler.execute("rustcodegraph_status", &Map::new());

        assert_ne!(res.is_error, Some(true));
        assert_eq!(await_count.load(Ordering::SeqCst), 0);
        assert!(first_text(&res).contains("RustCodeGraph Status"));
    }

    #[test]
    fn drops_the_gate_after_first_await_second_call_does_not_re_wait() {
        let mut fixture = Fixture::new();
        let await_count = Arc::new(AtomicUsize::new(0));
        let await_count_in_gate = Arc::clone(&await_count);
        fixture.handler.set_catch_up_gate(move || {
            await_count_in_gate.fetch_add(1, Ordering::SeqCst);
            thread::sleep(Duration::from_millis(20));
            Ok(())
        });

        let _ = fixture.execute_search("survivor");
        let before = await_count.load(Ordering::SeqCst);
        let _ = fixture.execute_search("survivor");

        assert_eq!(await_count.load(Ordering::SeqCst), before);
    }

    #[test]
    fn catch_up_reconciles_a_deleted_file_before_the_first_tool_call_sees_it() {
        let mut fixture = Fixture::new();
        fixture.temp.remove_src("deleted-later.ts");
        let root = fixture.temp.path().to_path_buf();

        fixture.handler.set_catch_up_gate(move || {
            let mut cg = CodeGraph::open_sync(&root)
                .map_err(|err| format!("failed to open catch-up CodeGraph: {err}"))?;
            let _ = cg.sync(IndexOptions::default());
            cg.close();
            Ok(())
        });

        let res = fixture.execute_search("deletedLater");

        assert_ne!(res.is_error, Some(true));
        assert!(!first_text(&res).contains("src/deleted-later.ts"));
    }

    #[test]
    fn catch_up_that_converges_the_project_to_0_files_clears_all_rows() {
        let mut fixture = Fixture::new();
        fixture.temp.remove_src("survivor.ts");
        fixture.temp.remove_src("deleted-later.ts");
        let root = fixture.temp.path().to_path_buf();

        fixture.handler.set_catch_up_gate(move || {
            let mut cg = CodeGraph::open_sync(&root)
                .map_err(|err| format!("failed to open catch-up CodeGraph: {err}"))?;
            let _ = cg.sync(IndexOptions::default());
            cg.close();
            Ok(())
        });

        let res = fixture.execute_search("survivor");

        assert_ne!(res.is_error, Some(true));
        assert_eq!(fixture.cg.get_stats().file_count, 0);
    }

    #[test]
    fn gate_that_rejects_does_not_break_the_tool_call() {
        let mut fixture = Fixture::new();
        fixture
            .handler
            .set_catch_up_gate(|| Err("simulated sync failure".to_string()));

        let res = fixture.execute_search("survivor");

        assert_ne!(res.is_error, Some(true));
        assert!(first_text(&res).contains("survivor"));
    }
}
