//! MCP tool input-size limits.
//!
//! This is the Rust port of `__tests__/integration/mcp-input-limits.test.ts`.
//! It covers the same DoS regression guard: MCP clients must not be able to
//! send unbounded payloads through tool inputs.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

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
        let root = env::temp_dir().join(format!(
            "codegraph-mcp-limits-{}-{unique}-{suffix}",
            std::process::id()
        ));
        fs::create_dir_all(root.join("src")).unwrap_or_else(|err| {
            panic!("failed to create fixture src dir {}: {err}", root.display())
        });
        Self { root }
    }

    fn path(&self) -> &Path {
        &self.root
    }

    fn write_src(&self, name: &str, contents: &str) {
        fs::write(self.root.join("src").join(name), contents)
            .unwrap_or_else(|err| panic!("failed to write fixture file {name}: {err}"));
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
    _temp: TempProject,
    cg: CodeGraph,
    handler: ToolHandler,
}

impl Fixture {
    fn new() -> Self {
        let temp = TempProject::new();
        temp.write_src("a.ts", "export function alpha(): number { return 1; }\n");

        let mut cg = CodeGraph::init_sync(temp.path()).expect("failed to initialize CodeGraph");
        let result = cg.index_all(IndexOptions::default());
        assert!(
            result.success,
            "index_all should succeed, errors: {:?}",
            result.errors
        );

        let mut handler = ToolHandler::new(true);
        handler.set_default_code_graph(&cg);

        Self {
            _temp: temp,
            cg,
            handler,
        }
    }

    fn execute(&mut self, tool: &str, args: Map<String, Value>) -> ToolResult {
        self.handler.execute(tool, &args)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.handler.close_all();
        self.cg.destroy();
    }
}

fn text(result: &ToolResult) -> &str {
    result
        .content
        .first()
        .map(|content| content.text.as_str())
        .unwrap_or("")
}

fn args(entries: &[(&str, Value)]) -> Map<String, Value> {
    entries
        .iter()
        .map(|(key, value)| ((*key).to_string(), value.clone()))
        .collect()
}

mod mcp_input_size_limits {
    use super::*;

    #[test]
    fn accepts_a_normal_sized_query() {
        let mut fixture = Fixture::new();

        let result = fixture.execute("rustcodegraph_search", args(&[("query", json!("alpha"))]));

        assert!(
            !result.is_error.unwrap_or(false),
            "normal query should not be an error: {result:?}"
        );
    }

    #[test]
    fn rejects_an_oversize_query_on_codegraph_search() {
        let mut fixture = Fixture::new();
        let huge = "a".repeat(20_000);

        let result = fixture.execute("rustcodegraph_search", args(&[("query", json!(huge))]));

        assert_eq!(result.is_error, Some(true));
        assert!(
            text(&result)
                .to_ascii_lowercase()
                .contains("maximum length"),
            "expected maximum length error, got: {}",
            text(&result)
        );
    }

    #[test]
    fn rejects_an_oversize_query_on_codegraph_explore() {
        let mut fixture = Fixture::new();
        let huge = "b".repeat(50_000);

        let result = fixture.execute("rustcodegraph_explore", args(&[("query", json!(huge))]));

        assert_eq!(result.is_error, Some(true));
        assert!(
            text(&result)
                .to_ascii_lowercase()
                .contains("maximum length"),
            "expected maximum length error, got: {}",
            text(&result)
        );
    }

    #[test]
    fn rejects_an_oversize_symbol_on_codegraph_callers() {
        let mut fixture = Fixture::new();
        let huge = "c".repeat(15_000);

        let result = fixture.execute("rustcodegraph_callers", args(&[("symbol", json!(huge))]));

        assert_eq!(result.is_error, Some(true));
        assert!(
            text(&result)
                .to_ascii_lowercase()
                .contains("maximum length"),
            "expected maximum length error, got: {}",
            text(&result)
        );
    }

    #[test]
    fn rejects_an_oversize_symbol_on_codegraph_impact() {
        let mut fixture = Fixture::new();
        let huge = "d".repeat(11_000);

        let result = fixture.execute("rustcodegraph_impact", args(&[("symbol", json!(huge))]));

        assert_eq!(result.is_error, Some(true));
        assert!(
            text(&result)
                .to_ascii_lowercase()
                .contains("maximum length"),
            "expected maximum length error, got: {}",
            text(&result)
        );
    }

    #[test]
    fn rejects_an_oversize_project_path() {
        let mut fixture = Fixture::new();
        let huge_path = format!("/tmp/{}", "x".repeat(5_000));

        let result = fixture.execute(
            "rustcodegraph_search",
            args(&[("query", json!("alpha")), ("projectPath", json!(huge_path))]),
        );

        assert_eq!(result.is_error, Some(true));
        assert!(
            text(&result).contains("projectPath"),
            "expected projectPath error, got: {}",
            text(&result)
        );
    }

    #[test]
    fn rejects_an_oversize_path_filter_on_codegraph_files() {
        let mut fixture = Fixture::new();
        let huge_path = format!("src/{}", "y".repeat(5_000));

        let result = fixture.execute("rustcodegraph_files", args(&[("path", json!(huge_path))]));

        assert_eq!(result.is_error, Some(true));
        assert!(
            text(&result).contains("path"),
            "expected path error, got: {}",
            text(&result)
        );
    }

    #[test]
    fn rejects_an_oversize_glob_pattern_on_codegraph_files() {
        let mut fixture = Fixture::new();
        let huge_pattern = "*".repeat(5_000);

        let result = fixture.execute(
            "rustcodegraph_files",
            args(&[("pattern", json!(huge_pattern))]),
        );

        assert_eq!(result.is_error, Some(true));
        assert!(
            text(&result).contains("pattern"),
            "expected pattern error, got: {}",
            text(&result)
        );
    }

    #[test]
    fn rejects_a_non_string_project_path() {
        let mut fixture = Fixture::new();

        let result = fixture.execute(
            "rustcodegraph_search",
            args(&[("query", json!("alpha")), ("projectPath", json!(12345))]),
        );

        assert_eq!(result.is_error, Some(true));
        assert!(
            text(&result).contains("projectPath"),
            "expected projectPath error, got: {}",
            text(&result)
        );
    }
}
