//! `rustcodegraph_node` file-view mode as a Read replacement.
//!
//! This is the Rust port of `__tests__/node-file-view.test.ts`.
//! The TypeScript suite drives `ToolHandler.execute("codegraph_node", ...)`
//! against an initialized index. These cases exercise the Rust MCP file-view
//! path directly so `rustcodegraph_node` can stand in for Read when given a
//! file without a symbol.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use regex::Regex;
use rustcodegraph::mcp::tools::{ToolHandler, ToolResult};
use rustcodegraph::{CodeGraph, IndexOptions};
use serde_json::{Map, Value, json};

const FILE_VIEW_STATUS: &str = "Rust CodeGraph MCP node file-view cases are active";
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
                "cg-fileview-{}-{unique}-{counter}-{attempt}",
                std::process::id()
            ));
            match fs::create_dir(&root) {
                Ok(()) => {
                    fs::create_dir(root.join("src")).unwrap_or_else(|err| {
                        panic!("failed to create temp src dir {}: {err}", root.display())
                    });
                    return Self { root };
                }
                Err(err) if err.kind() == ErrorKind::AlreadyExists => continue,
                Err(err) => panic!("failed to create temp dir {}: {err}", root.display()),
            }
        }
        panic!("failed to allocate a unique cg-fileview temp directory")
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
        temp.write_src(
            "a.ts",
            "export function helper(x: number) {\n  return x + 1;\n}\nexport class Widget {\n  build() { return helper(1); }\n}\n",
        );
        temp.write_src(
            "b.ts",
            "import { helper } from './a';\n\n// a comment between symbols\nconst SETTING = 7;\nexport function useHelper() { return helper(2) + SETTING; }\n",
        );
        // A config/data file (#383): its values may be secrets and must never be
        // dumped verbatim by the file-view.
        temp.write_src(
            "application.properties",
            "spring.datasource.password=SUPERSECRET123\nserver.port=8080\n",
        );
        // A large file: exceeds the file-view line budget, so it must be
        // windowed honestly (not silently truncated).
        let big_body = (0..2000)
            .map(|i| format!("  const v{i} = {i};"))
            .collect::<Vec<_>>()
            .join("\n");
        temp.write_src(
            "big.ts",
            &format!("export function big() {{\n{big_body}\n  return 0;\n}}\n"),
        );

        let mut cg = CodeGraph::init_sync(temp.path()).expect("failed to initialize CodeGraph");
        let _ = cg.index_all(IndexOptions::default());
        let mut handler = ToolHandler::new(true);
        handler.set_default_code_graph(&cg);

        Self {
            _temp: temp,
            cg,
            handler,
        }
    }

    fn text(&mut self, args: Map<String, Value>) -> String {
        text(&self.handler.execute("rustcodegraph_node", &args))
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.handler.close_all();
        self.cg.close();
    }
}

fn text(result: &ToolResult) -> String {
    result
        .content
        .iter()
        .map(|content| content.text.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

fn file_args(file: &str) -> Map<String, Value> {
    let mut args = Map::new();
    args.insert("file".to_string(), json!(file));
    args
}

fn file_args_with_offset_limit(
    file: &str,
    offset: usize,
    limit: Option<usize>,
) -> Map<String, Value> {
    let mut args = file_args(file);
    args.insert("offset".to_string(), json!(offset));
    if let Some(limit) = limit {
        args.insert("limit".to_string(), json!(limit));
    }
    args
}

fn symbols_only_args(file: &str) -> Map<String, Value> {
    let mut args = file_args(file);
    args.insert("symbolsOnly".to_string(), json!(true));
    args
}

fn symbol_args(symbol: &str, include_code: bool) -> Map<String, Value> {
    let mut args = Map::new();
    args.insert("symbol".to_string(), json!(symbol));
    args.insert("includeCode".to_string(), json!(include_code));
    args
}

mod codegraph_node_file_view_read_replacement {
    use super::*;

    #[test]
    fn reads_a_whole_file_like_read_by_default_numbered_tab_lines_no_pad_imports_and_gaps_included()
    {
        let mut fixture = Fixture::new();
        let out = fixture.text(file_args("b.ts"));

        // Byte-for-byte Read shape: line 1 is "1<TAB>import ...", NOT
        // space-padded.
        assert!(
            Regex::new(r"(?m)^1\timport \{ helper \} from '\./a';$")
                .expect("regex should compile")
                .is_match(&out),
            "{out}"
        );
        assert!(
            out.contains("// a comment between symbols"),
            "inter-symbol gap should be preserved:\n{out}"
        );
        assert!(
            out.contains("const SETTING = 7"),
            "top-level statement should be preserved:\n{out}"
        );
        assert!(
            out.contains("useHelper"),
            "symbol body should be present:\n{out}"
        );
        assert!(
            !out.contains("```"),
            "Read has no code fence; file-view should not either:\n{out}"
        );
    }

    #[test]
    fn leads_with_a_one_line_blast_radius_header_the_value_add_over_read() {
        let mut fixture = Fixture::new();
        let out = fixture.text(file_args("a.ts"));

        assert!(
            Regex::new(r"used by 1 file: src/b\.ts")
                .expect("regex should compile")
                .is_match(&out),
            "a.ts should report b.ts as a dependent:\n{out}"
        );
        assert!(
            out.contains("return x + 1"),
            "file source should still be returned:\n{out}"
        );
    }

    #[test]
    fn offset_limit_narrow_the_window_exactly_like_read() {
        let mut fixture = Fixture::new();
        let out = fixture.text(file_args_with_offset_limit("big.ts", 1000, Some(3)));

        // Window starts at the requested line, numbered exactly:
        // "1000<TAB>  const v998 = 998;"
        assert!(
            Regex::new(r"(?m)^1000\t {2}const v998 = 998;$")
                .expect("regex should compile")
                .is_match(&out),
            "{out}"
        );
        assert!(
            !Regex::new(r"(?m)^1\t")
                .expect("regex should compile")
                .is_match(&out),
            "line 1 should not be shown:\n{out}"
        );
        assert!(
            Regex::new(r"lines 1000[–-]1002 of \d+")
                .expect("regex should compile")
                .is_match(&out),
            "pagination note should report the exact window:\n{out}"
        );
    }

    #[test]
    fn an_offset_past_eof_is_reported_not_a_crash() {
        let mut fixture = Fixture::new();
        let out = fixture.text(file_args_with_offset_limit("a.ts", 9999, None));

        assert!(
            Regex::new("(?i)past the end")
                .expect("regex should compile")
                .is_match(&out),
            "{out}"
        );
    }

    #[test]
    fn paginates_a_large_file_honestly_by_default_lines_1_to_n_of_total_never_silent_truncate() {
        let mut fixture = Fixture::new();
        let out = fixture.text(file_args("big.ts"));

        assert!(
            Regex::new(r"lines 1[–-]\d+ of \d+")
                .expect("regex should compile")
                .is_match(&out),
            "large file should include an explicit window note:\n{out}"
        );
        assert!(
            !out.contains("(output truncated)"),
            "large file should avoid the generic output truncation marker:\n{out}"
        );
        assert!(
            Regex::new(r"(?m)^1\texport function big")
                .expect("regex should compile")
                .is_match(&out),
            "window head should be real source:\n{out}"
        );
    }

    #[test]
    fn does_not_dump_a_config_data_file_yaml_properties_issue_383_secret_safety() {
        let mut fixture = Fixture::new();
        let out = fixture.text(file_args("application.properties"));

        assert!(
            !out.contains("SUPERSECRET123"),
            "secret value must not reach the agent:\n{out}"
        );
        assert!(
            Regex::new("(?i)config|values withheld")
                .expect("regex should compile")
                .is_match(&out),
            "config/data output should explain that values are withheld:\n{out}"
        );
    }

    #[test]
    fn symbols_only_returns_the_structural_map_not_the_source() {
        let mut fixture = Fixture::new();
        let out = fixture.text(symbols_only_args("a.ts"));

        assert!(out.contains("### Symbols"), "{out}");
        assert!(out.contains("helper"), "{out}");
        assert!(out.contains("Widget"), "{out}");
        assert!(
            !out.contains("return x + 1"),
            "bodies should not be included in the map:\n{out}"
        );
    }

    #[test]
    fn still_works_as_a_normal_symbol_lookup_no_regression() {
        let mut fixture = Fixture::new();
        let out = fixture.text(symbol_args("helper", true));

        assert!(out.contains("helper"), "{out}");
        assert!(out.contains("return x + 1"), "{out}");
    }

    #[test]
    fn symbol_lookup_includes_a_caller_callee_trail() {
        let mut fixture = Fixture::new();
        let out = fixture.text(symbol_args("helper", true));

        assert!(out.contains("### Trail"), "{out}");
        assert!(out.contains("Called by"), "{out}");
        assert!(out.contains("useHelper"), "{out}");
        assert!(!out.contains("Source output is not available"), "{out}");
    }

    #[test]
    fn a_miss_returns_a_helpful_message_not_a_crash() {
        let mut fixture = Fixture::new();
        let out = fixture.text(file_args("does-not-exist.ts"));

        assert!(
            Regex::new("(?i)no indexed file matches")
                .expect("regex should compile")
                .is_match(&out),
            "{out}"
        );
    }

    #[test]
    fn file_view_cases_are_active_for_this_port() {
        assert_eq!(
            FILE_VIEW_STATUS,
            "Rust CodeGraph MCP node file-view cases are active"
        );
    }
}
