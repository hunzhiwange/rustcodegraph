//! `rustcodegraph_explore` blast-radius section.
//!
//! This is the Rust port of `__tests__/explore-blast-radius.test.ts`.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use regex::Regex;
use rustcodegraph::mcp::tools::{ToolHandler, ToolResult};
use rustcodegraph::{CodeGraph, IndexOptions};
use serde_json::{Map, Value, json};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

struct TempProject {
    root: PathBuf,
}

impl TempProject {
    fn new() -> Self {
        for attempt in 0..100 {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time should be after Unix epoch")
                .as_nanos();
            let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
            let root = std::env::temp_dir().join(format!(
                "codegraph-blast-{}-{unique}-{counter}-{attempt}",
                std::process::id()
            ));
            match fs::create_dir(&root) {
                Ok(()) => {
                    fs::create_dir(root.join("src")).unwrap_or_else(|err| {
                        panic!(
                            "failed to create fixture src directory {}: {err}",
                            root.display()
                        )
                    });
                    return Self { root };
                }
                Err(err) if err.kind() == ErrorKind::AlreadyExists => continue,
                Err(err) => panic!("failed to create fixture dir {}: {err}", root.display()),
            }
        }
        panic!("failed to allocate a unique blast-radius temp directory")
    }

    fn path(&self) -> &Path {
        &self.root
    }

    fn write_src(&self, name: &str, contents: &str) {
        let path = self.root.join("src").join(name);
        fs::write(&path, contents)
            .unwrap_or_else(|err| panic!("failed to write fixture {}: {err}", path.display()));
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

        // `target` is depended on by a sibling (caller) and a test file.
        temp.write_src(
            "feature.ts",
            "export function target() { return 1; }\n\
export function caller() { return target(); }\n",
        );
        temp.write_src(
            "feature.test.ts",
            "import { target } from './feature';\n\
export function checkTarget() { return target(); }\n",
        );
        // A leaf with no dependents -- must NOT show up in the blast radius.
        temp.write_src("leaf.ts", "export function lonelyLeaf() { return 42; }\n");

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

    fn explore(&mut self, query: &str) -> String {
        let result = self.handler.execute("rustcodegraph_explore", &args(query));
        first_text(&result).to_string()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.handler.close_all();
        self.cg.destroy();
    }
}

fn first_text(result: &ToolResult) -> &str {
    result
        .content
        .first()
        .map(|content| content.text.as_str())
        .unwrap_or("")
}

fn args(query: &str) -> Map<String, Value> {
    let mut args = Map::new();
    args.insert("query".to_string(), json!(query));
    args
}

mod codegraph_explore_blast_radius {
    use super::*;

    #[test]
    fn lists_dependents_locations_only_and_covering_tests_for_an_entry_symbol() {
        let mut fixture = Fixture::new();
        let text = fixture.explore("target");

        assert!(text.contains("### Blast radius"));
        assert!(text.contains("`target`"));
        assert!(
            Regex::new("caller").unwrap().is_match(&text),
            "a caller count is reported"
        );
        // It names WHERE (the caller file) -- not the caller's source body.
        assert!(text.contains("feature.ts"));
        // Test coverage is surfaced (either the covering test file, or the warning).
        assert!(
            Regex::new(r"tests:.*feature\.test\.ts|no covering tests")
                .unwrap()
                .is_match(&text)
        );
    }

    #[test]
    fn flow_section_shows_the_static_call_path_between_named_symbols() {
        let mut fixture = Fixture::new();
        let text = fixture.explore("caller target");

        assert!(text.contains("## Flow"), "{text}");
        assert!(
            Regex::new(r"caller .*--calls--> target")
                .unwrap()
                .is_match(&text),
            "{text}"
        );
    }

    #[test]
    fn omits_symbols_that_have_no_dependents_from_the_blast_radius() {
        let mut fixture = Fixture::new();
        let text = fixture.explore("lonelyLeaf");

        // lonelyLeaf has zero callers -- it must never appear under a
        // blast-radius bullet.
        assert!(
            !Regex::new(r"(?s)Blast radius.*`lonelyLeaf`")
                .unwrap()
                .is_match(&text)
        );
    }
}
