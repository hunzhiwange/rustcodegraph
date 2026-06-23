//! Value-reference edges: same-file `references` edges from a reader symbol to
//! the file-scope const/var it reads.
//!
//! This is the Rust port of `__tests__/value-reference-edges.test.ts`.

use std::collections::HashSet;
use std::env;
use std::fs;
use std::panic;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use rustcodegraph::types::{Edge, EdgeKind, NodeKind};
use rustcodegraph::{CodeGraph, IndexOptions};

const RUST_VALUE_REFS_ENV: &str = "RUSTCODEGRAPH_VALUE_REFS";
const TS_VALUE_REFS_ENV: &str = "RUSTCODEGRAPH_VALUE_REFS";
static VALUE_REFS_ENV_LOCK: Mutex<()> = Mutex::new(());
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

struct TempProject {
    root: PathBuf,
}

impl TempProject {
    fn new() -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after the Unix epoch")
            .as_nanos();
        let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let root = env::temp_dir().join(format!(
            "codegraph-valueref-{}-{nanos}-{counter}",
            std::process::id()
        ));
        fs::create_dir_all(&root).expect("failed to create temp project directory");
        Self { root }
    }

    fn path(&self) -> &Path {
        &self.root
    }
}

impl Drop for TempProject {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn write_lines(project: &TempProject, relative_path: &str, lines: &[&str]) {
    fs::write(project.path().join(relative_path), lines.join("\n"))
        .unwrap_or_else(|err| panic!("failed to write fixture {relative_path}: {err}"));
}

fn index_unlocked(project_root: &Path) -> CodeGraph {
    // The TS source passes an init config with include/exclude globs. The Rust
    // facade does not expose config injection yet, so this preserves the same
    // init-then-index boundary and leaves config fidelity to the backend wiring.
    let mut cg = CodeGraph::init_sync(project_root).expect("failed to initialize CodeGraph");
    let result = cg.index_all(IndexOptions::default());
    assert!(
        result.success,
        "index_all should succeed, errors: {:?}",
        result.errors
    );
    cg
}

fn index(project_root: &Path) -> CodeGraph {
    let _guard = VALUE_REFS_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    index_unlocked(project_root)
}

fn is_value_ref(edge: &Edge) -> bool {
    edge.kind == EdgeKind::References
        && edge
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("valueRef"))
            .and_then(|value| value.as_bool())
            == Some(true)
}

fn value_ref_readers(cg: &mut CodeGraph, const_name: &str) -> Vec<String> {
    // Aggregate across ALL nodes of this name: a conditionally-defined module
    // const (`try: X=...; except: X=...`) has more than one, and the edge targets
    // whichever one ended up in the target map.
    let targets = cg
        .search_nodes(const_name, None)
        .into_iter()
        .map(|result| result.node)
        .filter(|node| node.name == const_name)
        .collect::<Vec<_>>();

    let mut readers = HashSet::new();
    for target in targets {
        for edge in cg.get_incoming_edges(&target.id) {
            if !is_value_ref(&edge) {
                continue;
            }
            if let Some(reader) = cg.get_node(&edge.source) {
                readers.insert(reader.name);
            }
        }
    }

    let mut readers = readers.into_iter().collect::<Vec<_>>();
    readers.sort();
    readers
}

fn assert_contains_all(actual: &[String], expected: &[&str]) {
    for expected_name in expected {
        assert!(
            actual
                .iter()
                .any(|actual_name| actual_name == expected_name),
            "expected {actual:?} to contain {expected_name:?}"
        );
    }
}

fn assert_empty(actual: &[String]) {
    assert!(actual.is_empty(), "expected no readers, got {actual:?}");
}

fn restore_env(name: &str, previous: Option<std::ffi::OsString>) {
    unsafe {
        if let Some(value) = previous {
            env::set_var(name, value);
        } else {
            env::remove_var(name);
        }
    }
}

fn with_value_refs_disabled(test: impl FnOnce()) {
    let _guard = VALUE_REFS_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let previous_rust = env::var_os(RUST_VALUE_REFS_ENV);
    let previous_ts = env::var_os(TS_VALUE_REFS_ENV);
    unsafe {
        env::set_var(RUST_VALUE_REFS_ENV, "0");
        env::set_var(TS_VALUE_REFS_ENV, "0");
    }

    let result = panic::catch_unwind(panic::AssertUnwindSafe(test));

    restore_env(RUST_VALUE_REFS_ENV, previous_rust);
    restore_env(TS_VALUE_REFS_ENV, previous_ts);

    if let Err(payload) = result {
        panic::resume_unwind(payload);
    }
}

mod value_reference_edges {
    use super::*;

    #[test]
    fn edges_same_file_readers_to_the_file_scope_const_they_read_default_on() {
        let project = TempProject::new();
        write_lines(
            &project,
            "config.ts",
            &[
                "export const TABLE_CONFIG = { rows: 10, cols: 4 };",
                "export function rowCount() { return TABLE_CONFIG.rows; }",
                "export function describeTable() { return `${TABLE_CONFIG.rows}x${TABLE_CONFIG.cols}`; }",
                "export const HEADER = TABLE_CONFIG.cols;",
            ],
        );
        let mut cg = index(project.path());

        let readers = value_ref_readers(&mut cg, "TABLE_CONFIG");
        // rowCount, describeTable, and the HEADER const all read TABLE_CONFIG.
        assert_contains_all(&readers, &["rowCount", "describeTable", "HEADER"]);
    }

    #[test]
    fn surfaces_those_readers_in_the_impact_radius_of_the_const() {
        let project = TempProject::new();
        write_lines(
            &project,
            "palette.ts",
            &[
                "export const COLOR_PALETTE = { red: \"#f00\", blue: \"#00f\" };",
                "export function pickRed() { return COLOR_PALETTE.red; }",
            ],
        );
        let mut cg = index(project.path());

        let target = cg
            .search_nodes("COLOR_PALETTE", None)
            .into_iter()
            .map(|result| result.node)
            .find(|node| node.name == "COLOR_PALETTE")
            .expect("COLOR_PALETTE should be indexed");
        let impacted = cg
            .get_impact_radius(&target.id, 3)
            .nodes
            .into_values()
            .map(|node| node.name)
            .collect::<Vec<_>>();
        assert_contains_all(&impacted, &["pickRed"]);
    }

    #[test]
    fn does_not_edge_a_shadowed_const_inner_re_declaration_makes_the_name_ambiguous() {
        let project = TempProject::new();
        // The Emscripten/bundled pattern: a file-scope `const Module`
        // re-declared as an inner `var Module` / param. Nested readers resolve to
        // the inner binding, so a file-scope edge would be a false positive.
        write_lines(
            &project,
            "bundled.ts",
            &[
                "const Module = (function () {",
                "  return function (Module) {",
                "    var Module = typeof Module !== \"undefined\" ? Module : {};",
                "    function locate() { return Module.path; }",
                "    function getFunc() { return Module.lookup; }",
                "    return { locate, getFunc };",
                "  };",
                "})();",
                "export default Module;",
            ],
        );
        let mut cg = index(project.path());

        assert_empty(&value_ref_readers(&mut cg, "Module"));
    }

    #[test]
    fn edges_readers_that_use_the_const_only_inside_jsx_tsx() {
        let project = TempProject::new();
        // The tsx-specific path: the const is read only inside JSX expressions,
        // so the reader-scan must descend into the JSX subtree to find it.
        write_lines(
            &project,
            "widget.tsx",
            &[
                "export const THEME_TOKENS = { color: \"red\", size: 12 };",
                "export function Label() {",
                "  return <span style={{ color: THEME_TOKENS.color }}>hi</span>;",
                "}",
                "export const Box = () => <div data-size={THEME_TOKENS.size} />;",
            ],
        );
        let mut cg = index(project.path());

        assert_contains_all(
            &value_ref_readers(&mut cg, "THEME_TOKENS"),
            &["Label", "Box"],
        );
    }

    #[test]
    fn edges_same_file_readers_to_a_module_level_const_static_rust() {
        let project = TempProject::new();
        write_lines(
            &project,
            "lib.rs",
            &[
                "const MAX_RETRIES: u32 = 3;",
                "static DEFAULT_LABEL: &str = \"prod\";",
                "",
                "fn retry() -> u32 { MAX_RETRIES }",
                "fn label() -> &'static str { DEFAULT_LABEL }",
            ],
        );
        let mut cg = index(project.path());

        assert_contains_all(&value_ref_readers(&mut cg, "MAX_RETRIES"), &["retry"]);
        assert_contains_all(&value_ref_readers(&mut cg, "DEFAULT_LABEL"), &["label"]);
    }

    #[test]
    fn does_not_edge_a_rust_const_shadowed_by_a_local_let_of_the_same_name() {
        let project = TempProject::new();
        write_lines(
            &project,
            "shadow.rs",
            &[
                "const TIMEOUT: u32 = 30;",
                "",
                "fn uses_const() -> u32 { TIMEOUT }",
                "fn shadows() -> u32 {",
                "    let TIMEOUT = 5;",
                "    TIMEOUT",
                "}",
            ],
        );
        let mut cg = index(project.path());

        assert_empty(&value_ref_readers(&mut cg, "TIMEOUT"));
    }

    #[test]
    fn edges_same_file_readers_to_a_package_level_const_var_go() {
        let project = TempProject::new();
        write_lines(
            &project,
            "main.go",
            &[
                "package main",
                "",
                "const MaxRetries = 3",
                "var DefaultLabels = map[string]string{\"env\": \"prod\"}",
                "",
                "func retry() int { return MaxRetries }",
                "func labels() map[string]string { return DefaultLabels }",
            ],
        );
        let mut cg = index(project.path());

        assert_contains_all(&value_ref_readers(&mut cg, "MaxRetries"), &["retry"]);
        assert_contains_all(&value_ref_readers(&mut cg, "DefaultLabels"), &["labels"]);
    }

    #[test]
    fn does_not_edge_a_go_package_const_shadowed_by_a_local_short_var_of_the_same_name() {
        let project = TempProject::new();
        // The local read resolves to the inner binding, so a file-scope edge
        // would be a false positive.
        write_lines(
            &project,
            "shadow.go",
            &[
                "package main",
                "",
                "const Timeout = 30",
                "",
                "func usesConst() int { return Timeout }",
                "func shadows() int {",
                "\tTimeout := 5",
                "\treturn Timeout",
                "}",
            ],
        );
        let mut cg = index(project.path());

        assert_empty(&value_ref_readers(&mut cg, "Timeout"));
    }

    #[test]
    fn keeps_a_conditionally_defined_module_const_try_except_not_a_shadow_python() {
        let project = TempProject::new();
        // HAS_SSL is defined twice at module scope. It is one logical const, not
        // a shadow, so its reader must stay edged and the halves must not edge
        // each other.
        write_lines(
            &project,
            "cond.py",
            &[
                "try:",
                "\tHAS_SSL = True",
                "except ImportError:",
                "\tHAS_SSL = False",
                "",
                "def uses_ssl():",
                "\treturn HAS_SSL",
            ],
        );
        let mut cg = index(project.path());

        assert_eq!(value_ref_readers(&mut cg, "HAS_SSL"), vec!["uses_ssl"]);
    }

    #[test]
    fn edges_readers_to_a_top_level_and_a_class_internal_constant_ruby() {
        let project = TempProject::new();
        write_lines(
            &project,
            "app.rb",
            &[
                "MAX_RETRIES = 3",
                "",
                "def retry_count",
                "  MAX_RETRIES",
                "end",
                "",
                "class Config",
                "  TIMEOUT = 30",
                "  def self.get_timeout",
                "    TIMEOUT",
                "  end",
                "  def describe",
                "    \"timeout=#{TIMEOUT}\"",
                "  end",
                "end",
            ],
        );
        let mut cg = index(project.path());

        assert_contains_all(&value_ref_readers(&mut cg, "MAX_RETRIES"), &["retry_count"]);
        assert_contains_all(
            &value_ref_readers(&mut cg, "TIMEOUT"),
            &["get_timeout", "describe"],
        );
    }

    #[test]
    fn edges_same_file_readers_to_a_file_scope_const_table_c() {
        let project = TempProject::new();
        // C keeps shareable values at file scope as `static const`: scalars and
        // pointer/array lookup tables.
        write_lines(
            &project,
            "config.c",
            &[
                "static const int MAX_ITEMS = 100;",
                "static const char *const STATUS_NAMES[] = { \"ok\", \"fail\", \"pending\" };",
                "",
                "int capped(int n) { return n > MAX_ITEMS ? MAX_ITEMS : n; }",
                "const char *label(int i) { return STATUS_NAMES[i]; }",
            ],
        );
        let mut cg = index(project.path());

        assert_contains_all(&value_ref_readers(&mut cg, "MAX_ITEMS"), &["capped"]);
        assert_contains_all(&value_ref_readers(&mut cg, "STATUS_NAMES"), &["label"]);
    }

    #[test]
    fn does_not_edge_a_c_file_const_shadowed_by_a_function_local_of_the_same_name() {
        let project = TempProject::new();
        write_lines(
            &project,
            "shadow.c",
            &[
                "static const int TIMEOUT = 30;",
                "",
                "int uses_const(void) { return TIMEOUT; }",
                "int shadows(void) {",
                "    int TIMEOUT = 5;",
                "    return TIMEOUT;",
                "}",
            ],
        );
        let mut cg = index(project.path());

        assert_empty(&value_ref_readers(&mut cg, "TIMEOUT"));
    }

    #[test]
    fn does_not_mint_a_value_target_from_a_macro_prefixed_c_prototype_return_type_misparse() {
        let project = TempProject::new();
        write_lines(
            &project,
            "api.c",
            &[
                "typedef enum { CURLE_OK, CURLE_FAIL } CURLcode;",
                "CURL_EXTERN CURLcode curl_easy_init(int x);",
                "CURL_EXTERN CURLcode curl_easy_setopt(int y);",
                "",
                "static const int REAL_LIMIT = 42;",
                "int use_real(void) { return REAL_LIMIT; }",
            ],
        );
        let mut cg = index(project.path());

        let curlcode_values = cg
            .search_nodes("CURLcode", None)
            .into_iter()
            .map(|result| result.node)
            .filter(|node| {
                node.name == "CURLcode"
                    && matches!(node.kind, NodeKind::Constant | NodeKind::Variable)
            })
            .collect::<Vec<_>>();
        assert!(
            curlcode_values.is_empty(),
            "CURLcode should not be extracted as a value target: {curlcode_values:?}"
        );
        assert_contains_all(&value_ref_readers(&mut cg, "REAL_LIMIT"), &["use_real"]);
    }

    #[test]
    fn edges_same_file_methods_to_a_class_scope_static_final_constant_java() {
        let project = TempProject::new();
        write_lines(
            &project,
            "Limits.java",
            &[
                "class Limits {",
                "  public static final int MAX_ITEMS = 100;",
                "  static final String[] STATUS_NAMES = { \"ok\", \"fail\" };",
                "  final int instanceId = 1;",
                "  int capped(int n) { return n > MAX_ITEMS ? MAX_ITEMS : n; }",
                "  String label(int i) { return STATUS_NAMES[i]; }",
                "  int id() { return instanceId; }",
                "}",
            ],
        );
        let mut cg = index(project.path());

        assert_contains_all(&value_ref_readers(&mut cg, "MAX_ITEMS"), &["capped"]);
        assert_contains_all(&value_ref_readers(&mut cg, "STATUS_NAMES"), &["label"]);
        assert_empty(&value_ref_readers(&mut cg, "instanceId"));
    }

    #[test]
    fn does_not_edge_a_java_class_const_shadowed_by_a_method_local_of_the_same_name() {
        let project = TempProject::new();
        write_lines(
            &project,
            "Shadow.java",
            &[
                "class Shadow {",
                "  static final int TIMEOUT = 30;",
                "  int usesConst() { return TIMEOUT; }",
                "  int shadows() { int TIMEOUT = 5; return TIMEOUT; }",
                "}",
            ],
        );
        let mut cg = index(project.path());

        assert_empty(&value_ref_readers(&mut cg, "TIMEOUT"));
    }

    #[test]
    fn edges_same_file_methods_to_a_class_const_static_readonly_csharp() {
        let project = TempProject::new();
        write_lines(
            &project,
            "Limits.cs",
            &[
                "class Limits {",
                "  const int MAX_ITEMS = 100;",
                "  static readonly string[] STATUS_NAMES = { \"ok\", \"fail\" };",
                "  readonly int instanceId = 1;",
                "  int Capped(int n) { return n > MAX_ITEMS ? MAX_ITEMS : n; }",
                "  string Label(int i) { return STATUS_NAMES[i]; }",
                "  int Id() { return instanceId; }",
                "}",
            ],
        );
        let mut cg = index(project.path());

        assert_contains_all(&value_ref_readers(&mut cg, "MAX_ITEMS"), &["Capped"]);
        assert_contains_all(&value_ref_readers(&mut cg, "STATUS_NAMES"), &["Label"]);
        assert_empty(&value_ref_readers(&mut cg, "instanceId"));
    }

    #[test]
    fn does_not_edge_a_csharp_class_const_shadowed_by_a_method_local_of_the_same_name() {
        let project = TempProject::new();
        write_lines(
            &project,
            "Shadow.cs",
            &[
                "class Shadow {",
                "  const int TIMEOUT = 30;",
                "  int UsesConst() { return TIMEOUT; }",
                "  int Shadows() { int TIMEOUT = 5; return TIMEOUT; }",
                "}",
            ],
        );
        let mut cg = index(project.path());

        assert_empty(&value_ref_readers(&mut cg, "TIMEOUT"));
    }

    #[test]
    fn edges_same_file_readers_to_a_top_level_and_class_const_including_self_and_class_php() {
        let project = TempProject::new();
        write_lines(
            &project,
            "Config.php",
            &[
                "<?php",
                "const APP_VERSION = \"1.0\";",
                "class Config {",
                "  const MAX_ITEMS = 100;",
                "  const STATUS_NAMES = [\"ok\", \"fail\"];",
                "  public static $counter = 0;",
                "  function capped($n) { return $n > self::MAX_ITEMS ? self::MAX_ITEMS : $n; }",
                "  function label($i) { return Config::STATUS_NAMES[$i]; }",
                "  function version() { return APP_VERSION; }",
                "}",
            ],
        );
        let mut cg = index(project.path());

        assert_contains_all(&value_ref_readers(&mut cg, "MAX_ITEMS"), &["capped"]);
        assert_contains_all(&value_ref_readers(&mut cg, "STATUS_NAMES"), &["label"]);
        assert_contains_all(&value_ref_readers(&mut cg, "APP_VERSION"), &["version"]);
        assert_empty(&value_ref_readers(&mut cg, "counter"));
    }

    #[test]
    fn edges_readers_to_a_top_level_and_object_scope_val_not_a_class_instance_val_scala() {
        let project = TempProject::new();
        write_lines(
            &project,
            "Demo.scala",
            &[
                "val AppVersion = \"1.0\"",
                "object Config {",
                "  val TIMEOUT_MS = 30",
                "  val STATUS_NAMES = List(\"ok\", \"fail\")",
                "  def capped(n: Int): Int = if (n > TIMEOUT_MS) TIMEOUT_MS else n",
                "  def label(i: Int): String = STATUS_NAMES(i)",
                "}",
                "class Widget {",
                "  val MaxItems = 100",
                "  def within(n: Int): Int = if (n < MaxItems) n else MaxItems",
                "}",
            ],
        );
        let mut cg = index(project.path());

        assert_contains_all(&value_ref_readers(&mut cg, "TIMEOUT_MS"), &["capped"]);
        assert_contains_all(&value_ref_readers(&mut cg, "STATUS_NAMES"), &["label"]);
        assert_empty(&value_ref_readers(&mut cg, "MaxItems"));
    }

    #[test]
    fn does_not_edge_a_scala_object_val_shadowed_by_a_method_local_val_of_the_same_name() {
        let project = TempProject::new();
        write_lines(
            &project,
            "Shadow.scala",
            &[
                "object Config {",
                "  val TIMEOUT = 30",
                "  def usesConst(): Int = TIMEOUT",
                "  def shadows(): Int = { val TIMEOUT = 5; TIMEOUT }",
                "}",
            ],
        );
        let mut cg = index(project.path());

        assert_empty(&value_ref_readers(&mut cg, "TIMEOUT"));
    }

    #[test]
    fn edges_readers_to_top_level_object_and_companion_object_constants_not_a_class_val_kotlin() {
        let project = TempProject::new();
        write_lines(
            &project,
            "Demo.kt",
            &[
                "const val TOP_LEVEL_MAX = 100",
                "object Config {",
                "  const val TIMEOUT_MS = 30",
                "  val STATUS_NAMES = listOf(\"ok\", \"fail\")",
                "  fun capped(n: Int): Int = if (n > TIMEOUT_MS) TIMEOUT_MS else n",
                "  fun label(i: Int): String = STATUS_NAMES[i]",
                "}",
                "class Widget {",
                "  companion object { const val MAX_RETRIES = 3 }",
                "  val instanceField = 1",
                "  fun retries(): Int = MAX_RETRIES",
                "  fun within(n: Int): Int = if (n < TOP_LEVEL_MAX) n else TOP_LEVEL_MAX",
                "}",
            ],
        );
        let mut cg = index(project.path());

        assert_contains_all(&value_ref_readers(&mut cg, "STATUS_NAMES"), &["label"]);
        assert_contains_all(&value_ref_readers(&mut cg, "MAX_RETRIES"), &["retries"]);
        assert_contains_all(&value_ref_readers(&mut cg, "TOP_LEVEL_MAX"), &["within"]);
        assert_empty(&value_ref_readers(&mut cg, "instanceField"));
    }

    #[test]
    fn does_not_edge_a_kotlin_object_const_shadowed_by_a_method_local_val_of_the_same_name() {
        let project = TempProject::new();
        write_lines(
            &project,
            "Shadow.kt",
            &[
                "object Config {",
                "  const val TIMEOUT = 30",
                "  fun usesConst(): Int = TIMEOUT",
                "  fun shadows(): Int { val TIMEOUT = 5; return TIMEOUT }",
                "}",
            ],
        );
        let mut cg = index(project.path());

        assert_empty(&value_ref_readers(&mut cg, "TIMEOUT"));
    }

    #[test]
    fn edges_readers_to_a_top_level_let_and_static_let_in_enum_struct_not_an_instance_let_swift() {
        let project = TempProject::new();
        write_lines(
            &project,
            "Demo.swift",
            &[
                "let topLevelMax = 100",
                "enum Constants {",
                "  static let TIMEOUT_MS = 30",
                "  static let STATUS_NAMES = [\"ok\", \"fail\"]",
                "}",
                "struct Widget {",
                "  static let MAX_RETRIES = 3",
                "  let instanceField = 1",
                "  func retries() -> Int { return Widget.MAX_RETRIES }",
                "  func within(_ n: Int) -> Int { return n < topLevelMax ? n : topLevelMax }",
                "}",
                "func labels(_ i: Int) -> String { return Constants.STATUS_NAMES[i] }",
            ],
        );
        let mut cg = index(project.path());

        assert_contains_all(&value_ref_readers(&mut cg, "STATUS_NAMES"), &["labels"]);
        assert_contains_all(&value_ref_readers(&mut cg, "MAX_RETRIES"), &["retries"]);
        assert_contains_all(&value_ref_readers(&mut cg, "topLevelMax"), &["within"]);
        assert_empty(&value_ref_readers(&mut cg, "instanceField"));
    }

    #[test]
    fn does_not_edge_a_swift_static_const_shadowed_by_a_function_local_let_of_the_same_name() {
        let project = TempProject::new();
        write_lines(
            &project,
            "Shadow.swift",
            &[
                "enum Config {",
                "  static let TIMEOUT = 30",
                "  static func usesConst() -> Int { return TIMEOUT }",
                "  static func shadows() -> Int { let TIMEOUT = 5; return TIMEOUT }",
                "}",
            ],
        );
        let mut cg = index(project.path());

        assert_empty(&value_ref_readers(&mut cg, "TIMEOUT"));
    }

    #[test]
    fn edges_readers_to_a_top_level_const_and_a_class_static_const_final_dart() {
        let project = TempProject::new();
        write_lines(
            &project,
            "demo.dart",
            &[
                "const TOP_LEVEL_MAX = 100;",
                "class Config {",
                "  static const TIMEOUT_MS = 30;",
                "  static final STATUS_NAMES = [\"ok\", \"fail\"];",
                "  final int instanceField = 1;",
                "  int capped(int n) => n > TIMEOUT_MS ? TIMEOUT_MS : n;",
                "  String label(int i) { return STATUS_NAMES[i]; }",
                "  int withinLimit(int n) => n < TOP_LEVEL_MAX ? n : TOP_LEVEL_MAX;",
                "}",
            ],
        );
        let mut cg = index(project.path());

        assert_contains_all(&value_ref_readers(&mut cg, "TIMEOUT_MS"), &["capped"]);
        assert_contains_all(&value_ref_readers(&mut cg, "STATUS_NAMES"), &["label"]);
        assert_contains_all(
            &value_ref_readers(&mut cg, "TOP_LEVEL_MAX"),
            &["withinLimit"],
        );
        assert_empty(&value_ref_readers(&mut cg, "instanceField"));
    }

    #[test]
    fn does_not_edge_a_dart_const_shadowed_by_a_method_local_const_of_the_same_name() {
        let project = TempProject::new();
        write_lines(
            &project,
            "shadow.dart",
            &[
                "const TIMEOUT = 30;",
                "class C {",
                "  int usesConst() => TIMEOUT;",
                "  int shadows() { const TIMEOUT = 5; return TIMEOUT; }",
                "}",
            ],
        );
        let mut cg = index(project.path());

        assert_empty(&value_ref_readers(&mut cg, "TIMEOUT"));
    }

    #[test]
    fn edges_same_file_functions_to_a_unit_scope_const_pascal() {
        let project = TempProject::new();
        write_lines(
            &project,
            "demo.pas",
            &[
                "unit Demo;",
                "interface",
                "const",
                "  MAX_ITEMS = 100;",
                "  APP_NAME = 'MyApp';",
                "implementation",
                "function Capped(n: Integer): Integer;",
                "begin",
                "  if n > MAX_ITEMS then Capped := MAX_ITEMS else Capped := n;",
                "end;",
                "function AppLabel: string;",
                "begin",
                "  AppLabel := APP_NAME;",
                "end;",
                "end.",
            ],
        );
        let mut cg = index(project.path());

        assert_contains_all(&value_ref_readers(&mut cg, "MAX_ITEMS"), &["Capped"]);
        assert_contains_all(&value_ref_readers(&mut cg, "APP_NAME"), &["AppLabel"]);
    }

    #[test]
    fn does_not_edge_a_pascal_unit_const_shadowed_by_a_function_local_const_of_the_same_name() {
        let project = TempProject::new();
        write_lines(
            &project,
            "shadow.pas",
            &[
                "unit Shadow;",
                "interface",
                "const",
                "  TIMEOUT = 30;",
                "implementation",
                "function UsesConst: Integer;",
                "begin",
                "  UsesConst := TIMEOUT;",
                "end;",
                "function Shadows: Integer;",
                "const TIMEOUT = 5;",
                "begin",
                "  Shadows := TIMEOUT;",
                "end;",
                "end.",
            ],
        );
        let mut cg = index(project.path());

        assert_empty(&value_ref_readers(&mut cg, "TIMEOUT"));
    }

    #[test]
    fn emits_nothing_when_codegraph_value_refs_0() {
        with_value_refs_disabled(|| {
            let project = TempProject::new();
            write_lines(
                &project,
                "config.ts",
                &[
                    "export const TABLE_CONFIG = { rows: 10 };",
                    "export function rowCount() { return TABLE_CONFIG.rows; }",
                ],
            );
            let mut cg = index_unlocked(project.path());

            assert_empty(&value_ref_readers(&mut cg, "TABLE_CONFIG"));
        });
    }
}
