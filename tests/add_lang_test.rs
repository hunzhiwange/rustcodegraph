use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rustcodegraph::add_lang::{
    DumpAstOptions, check_grammar_report, dump_ast_report, verify_extraction_report,
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

#[test]
fn check_grammar_uses_native_tree_sitter_parser_path() {
    let temp = TempDir::new("codegraph-add-lang-grammar");
    let sample = temp.path().join("sample.rs");
    fs::write(&sample, "pub fn alpha() -> i32 {\n    1\n}\n").expect("sample should be written");

    let report = check_grammar_report("rust", &sample, 3).expect("grammar check should run");

    assert_eq!(report.exit_code, 0);
    assert!(report.text.contains("grammar: rust"), "{}", report.text);
    assert!(
        report
            .text
            .contains("RESULT: PASS - grammar parses cleanly"),
        "{}",
        report.text
    );
}

#[test]
fn dump_ast_reports_named_nodes_and_frequency_counts() {
    let temp = TempDir::new("codegraph-add-lang-dump");
    let sample = temp.path().join("sample.rs");
    fs::write(
        &sample,
        "pub fn alpha(input: i32) -> i32 {\n    input + 1\n}\n",
    )
    .expect("sample should be written");

    let report = dump_ast_report(
        "rust",
        &sample,
        DumpAstOptions {
            max_depth: Some(4),
            show_all: false,
        },
    )
    .expect("AST dump should run");

    assert_eq!(report.exit_code, 0);
    assert!(report.text.contains("# AST for"), "{}", report.text);
    assert!(report.text.contains("function_item"), "{}", report.text);
    assert!(
        report.text.contains("# Node-type frequency"),
        "{}",
        report.text
    );
}

#[test]
fn verify_extraction_reads_rust_status_shape_from_sqlite() {
    let temp = TempDir::new("codegraph-add-lang-verify");
    fs::write(
        temp.path().join("lib.rs"),
        "pub fn alpha() {\n    beta();\n}\n\npub fn beta() {}\n",
    )
    .expect("fixture should be written");
    let mut cg = CodeGraph::init_sync(temp.path()).expect("CodeGraph should initialize");
    let index = cg.index_all(IndexOptions::default());
    assert!(index.success, "indexing should succeed: {index:?}");
    cg.close();

    let report =
        verify_extraction_report(temp.path(), "rust").expect("verification should inspect index");

    assert_eq!(report.exit_code, 0);
    assert!(report.text.contains("files=1"), "{}", report.text);
    assert!(report.text.contains("nodesByKind"), "{}", report.text);
    assert!(
        report
            .text
            .contains("RESULT: PASS - extraction looks healthy"),
        "{}",
        report.text
    );
}
