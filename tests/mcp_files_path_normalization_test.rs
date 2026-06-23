//! `codegraph_files` path-filter normalization (#426).
//!
//! Stored file paths are project-relative POSIX (e.g. "src/foo.ts"). Some
//! agents pass project-root variants like "/", ".", "./" or "" when they want
//! "the whole project", and Windows-style backslashes or leading "/" / "./"
//! prefixes when they want a subtree. The old filter used a plain
//! `startsWith(pathFilter)`, so any of those buried the agent at "no files
//! found" and pushed it back to Read/Glob -- the exact opencode regression in
//! #426. These tests pin every branch of the normalization.
//!
//! This is the Rust port of
//! `__tests__/mcp-files-path-normalization.test.ts`.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use rustcodegraph::mcp::tools::{ToolHandler, ToolResult};
use rustcodegraph::{CodeGraph, IndexOptions};
use serde_json::{Map, json};

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
            "codegraph-files-paths-{}-{unique}-{suffix}",
            std::process::id()
        ));
        fs::create_dir_all(root.join("src").join("components")).unwrap_or_else(|err| {
            panic!(
                "failed to create fixture components dir {}: {err}",
                root.display()
            )
        });
        fs::create_dir_all(root.join("tests")).unwrap_or_else(|err| {
            panic!(
                "failed to create fixture tests dir {}: {err}",
                root.display()
            )
        });
        fs::write(root.join("src").join("index.ts"), "export const x = 1;\n")
            .expect("failed to write src/index.ts fixture");
        fs::write(
            root.join("src").join("components").join("Button.ts"),
            "export const Button = () => 1;\n",
        )
        .expect("failed to write src/components/Button.ts fixture");
        fs::write(
            root.join("tests").join("a.test.ts"),
            "export const t = 1;\n",
        )
        .expect("failed to write tests/a.test.ts fixture");
        Self { root }
    }

    fn path(&self) -> &Path {
        &self.root
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

    fn listed(&mut self, path_filter: Option<&str>) -> String {
        let mut args = Map::new();
        if let Some(path_filter) = path_filter {
            args.insert("path".to_string(), json!(path_filter));
        }
        args.insert("format".to_string(), json!("flat"));
        args.insert("includeMetadata".to_string(), json!(false));

        text(&self.handler.execute("rustcodegraph_files", &args))
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.handler.close_all();
        self.cg.close();
    }
}

fn text(result: &ToolResult) -> String {
    assert!(
        result.is_error != Some(true),
        "tool should not fail: {:?}",
        result
    );
    result
        .content
        .first()
        .map(|content| content.text.clone())
        .unwrap_or_default()
}

fn assert_lists_whole_project(output: &str) {
    assert!(output.contains("src/index.ts"), "{output}");
    assert!(output.contains("src/components/Button.ts"), "{output}");
    assert!(output.contains("tests/a.test.ts"), "{output}");
}

macro_rules! rootish_case {
    ($name:ident, $filter:expr) => {
        #[test]
        fn $name() {
            let mut fixture = Fixture::new();
            let output = fixture.listed(Some($filter));
            assert_lists_whole_project(&output);
        }
    };
}

mod codegraph_files_path_normalization {
    use super::*;

    // Root-ish filters: every shape an agent might guess for "whole project"
    // must list the same files as no filter at all.
    rootish_case!(treats_path_slash_as_project_root, "/");
    rootish_case!(treats_path_dot_as_project_root, ".");
    rootish_case!(treats_path_dot_slash_as_project_root, "./");
    rootish_case!(treats_path_empty_string_as_project_root, "");
    rootish_case!(treats_path_backslash_as_project_root, "\\");
    rootish_case!(treats_path_double_slash_as_project_root, "//");
    rootish_case!(treats_path_dot_double_slash_as_project_root, ".//");

    #[test]
    fn matches_a_real_subdirectory_prefix() {
        let mut fixture = Fixture::new();
        let output = fixture.listed(Some("src"));
        assert!(output.contains("src/index.ts"), "{output}");
        assert!(output.contains("src/components/Button.ts"), "{output}");
        assert!(!output.contains("tests/a.test.ts"), "{output}");
    }

    #[test]
    fn tolerates_a_leading_slash_on_a_real_subdirectory() {
        let mut fixture = Fixture::new();
        let output = fixture.listed(Some("/src"));
        assert!(output.contains("src/index.ts"), "{output}");
        assert!(!output.contains("tests/a.test.ts"), "{output}");
    }

    #[test]
    fn tolerates_a_leading_dot_slash_on_a_real_subdirectory() {
        let mut fixture = Fixture::new();
        let output = fixture.listed(Some("./src"));
        assert!(output.contains("src/index.ts"), "{output}");
        assert!(!output.contains("tests/a.test.ts"), "{output}");
    }

    #[test]
    fn tolerates_a_trailing_slash_on_a_real_subdirectory() {
        let mut fixture = Fixture::new();
        let output = fixture.listed(Some("src/"));
        assert!(output.contains("src/index.ts"), "{output}");
        assert!(!output.contains("tests/a.test.ts"), "{output}");
    }

    #[test]
    fn normalizes_windows_backslashes() {
        let mut fixture = Fixture::new();
        let output = fixture.listed(Some("src\\components"));
        assert!(output.contains("src/components/Button.ts"), "{output}");
        assert!(!output.contains("src/index.ts"), "{output}");
    }

    // Old code matched on raw `startsWith`, so a filter "src" would also
    // return a sibling like "src-utils/...". The new code requires either an
    // exact match or a "<filter>/" boundary, so prefixes don't bleed.
    #[test]
    fn does_not_match_sibling_directories_that_share_a_prefix() {
        let mut fixture = Fixture::new();
        fs::create_dir_all(fixture.temp.path().join("src-utils"))
            .expect("failed to create src-utils fixture dir");
        fs::write(
            fixture.temp.path().join("src-utils").join("helper.ts"),
            "export const h = 1;\n",
        )
        .expect("failed to write src-utils/helper.ts fixture");
        let result = fixture.cg.index_all(IndexOptions::default());
        assert!(
            result.success,
            "index_all should succeed after adding src-utils, errors: {:?}",
            result.errors
        );

        let output = fixture.listed(Some("src"));
        assert!(output.contains("src/index.ts"), "{output}");
        assert!(!output.contains("src-utils/helper.ts"), "{output}");
    }
}
