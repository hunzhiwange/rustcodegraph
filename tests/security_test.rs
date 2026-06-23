//! Security tests.
//!
//! This is the Rust port of `__tests__/security.test.ts`.

use std::collections::HashMap;
use std::env;
use std::fs;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::ptr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};
use std::time::{SystemTime, UNIX_EPOCH};
use std::{panic, panic::AssertUnwindSafe};

use rustcodegraph::db::queries::QueryBuilder;
use rustcodegraph::db::sqlite_adapter::{
    PragmaOptions, RunResult, SqliteDatabase, SqliteParams, SqliteResult, SqliteRow,
    SqliteStatement, SqliteValue,
};
use rustcodegraph::extraction::grammars::is_source_file;
use rustcodegraph::extraction::index::scan_directory;
use rustcodegraph::mcp::tools::{ToolHandler, ToolResult, truncate_output};
use rustcodegraph::types::NodeKind;
use rustcodegraph::utils::{FileLock, validate_path_within_root, validate_project_path};
use rustcodegraph::{CodeGraph, IndexOptions};
use serde_json::{Map, Value, json};

const BACKEND_BLOCKER: &str = "Rust CodeGraph indexing/query backend is not wired yet";

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(prefix: &str) -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after Unix epoch")
            .as_nanos();
        let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = env::temp_dir().join(format!(
            "{prefix}-{}-{unique}-{counter}",
            std::process::id()
        ));
        fs::create_dir(&path)
            .unwrap_or_else(|err| panic!("failed to create temp dir {}: {err}", path.display()));
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn realpath(&self) -> PathBuf {
        fs::canonicalize(&self.path)
            .unwrap_or_else(|err| panic!("failed to realpath {}: {err}", self.path.display()))
    }

    fn join(&self, path: impl AsRef<Path>) -> PathBuf {
        self.path.join(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn text_args(items: &[(&str, Value)]) -> Map<String, Value> {
    items
        .iter()
        .map(|(key, value)| ((*key).to_string(), value.clone()))
        .collect()
}

fn first_text(result: &ToolResult) -> &str {
    result
        .content
        .first()
        .map(|content| content.text.as_str())
        .unwrap_or("")
}

fn assert_not_error(result: &ToolResult) {
    assert_ne!(result.is_error, Some(true), "{}", first_text(result));
}

fn panic_contains(payload: &(dyn std::any::Any + Send), expected: &str) -> bool {
    payload
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
        .is_some_and(|message| message.contains(expected))
}

fn block_on<F>(future: F) -> F::Output
where
    F: Future,
{
    let waker = unsafe { Waker::from_raw(noop_raw_waker()) };
    let mut cx = Context::from_waker(&waker);
    let mut future = Box::pin(future);

    loop {
        match Pin::new(&mut future).poll(&mut cx) {
            Poll::Ready(value) => return value,
            Poll::Pending => std::thread::yield_now(),
        }
    }
}

fn noop_raw_waker() -> RawWaker {
    unsafe fn clone(_: *const ()) -> RawWaker {
        noop_raw_waker()
    }
    unsafe fn wake(_: *const ()) {}
    unsafe fn wake_by_ref(_: *const ()) {}
    unsafe fn drop(_: *const ()) {}

    static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, wake, wake_by_ref, drop);
    RawWaker::new(ptr::null(), &VTABLE)
}

#[cfg(unix)]
fn symlink_file(target: &Path, link: &Path) -> bool {
    std::os::unix::fs::symlink(target, link).is_ok()
}

#[cfg(windows)]
fn symlink_file(target: &Path, link: &Path) -> bool {
    std::os::windows::fs::symlink_file(target, link).is_ok()
}

#[cfg(unix)]
fn symlink_dir(target: &Path, link: &Path) -> bool {
    std::os::unix::fs::symlink(target, link).is_ok()
}

#[cfg(windows)]
fn symlink_dir(target: &Path, link: &Path) -> bool {
    std::os::windows::fs::symlink_dir(target, link).is_ok()
}

mod file_lock {
    use super::*;

    #[test]
    fn should_acquire_and_release_a_lock() {
        let temp_dir = TempDir::new("codegraph-security-test");
        let lock_path = temp_dir.join("test.lock");
        let mut lock = FileLock::new(&lock_path);

        lock.acquire().expect("lock should acquire");

        assert!(lock_path.exists());
        let content = fs::read_to_string(&lock_path)
            .expect("lock file should be readable")
            .trim()
            .parse::<u32>()
            .expect("lock should contain a PID");
        assert_eq!(content, std::process::id());

        lock.release();
        assert!(!lock_path.exists());
    }

    #[test]
    fn should_prevent_double_acquisition_within_same_process() {
        let temp_dir = TempDir::new("codegraph-security-test");
        let lock_path = temp_dir.join("test.lock");
        let mut lock1 = FileLock::new(&lock_path);
        let mut lock2 = FileLock::new(&lock_path);

        lock1.acquire().expect("first lock should acquire");

        let err = lock2
            .acquire()
            .expect_err("second lock should fail while our PID is alive");
        assert!(
            err.to_string().contains("locked by another process"),
            "{err}"
        );

        lock1.release();
    }

    #[test]
    fn should_detect_and_remove_stale_locks_from_dead_processes() {
        let temp_dir = TempDir::new("codegraph-security-test");
        let lock_path = temp_dir.join("test.lock");
        fs::write(&lock_path, "99999999").expect("failed to write stale lock");
        let mut lock = FileLock::new(&lock_path);

        lock.acquire().expect("stale lock should be replaced");

        lock.release();
    }

    #[test]
    fn should_execute_function_with_with_lock() {
        let temp_dir = TempDir::new("codegraph-security-test");
        let lock_path = temp_dir.join("test.lock");
        let mut lock = FileLock::new(&lock_path);

        let result = lock
            .with_lock(|| {
                assert!(lock_path.exists());
                42
            })
            .expect("with_lock should complete");

        assert_eq!(result, 42);
        assert!(!lock_path.exists());
    }

    #[test]
    fn should_release_lock_even_if_function_throws() {
        let temp_dir = TempDir::new("codegraph-security-test");
        let lock_path = temp_dir.join("test.lock");
        let mut lock = FileLock::new(&lock_path);

        let result = panic::catch_unwind(AssertUnwindSafe(|| {
            let _ = lock.with_lock(|| panic!("test error"));
        }));

        let payload = result.expect_err("with_lock should resume the panic");
        assert!(panic_contains(payload.as_ref(), "test error"));
        assert!(!lock_path.exists());
    }

    #[test]
    fn should_execute_async_function_with_with_lock_async() {
        let temp_dir = TempDir::new("codegraph-security-test");
        let lock_path = temp_dir.join("test.lock");
        let mut lock = FileLock::new(&lock_path);

        let result = block_on(lock.with_lock_async(|| async {
            assert!(lock_path.exists());
            "async-result"
        }))
        .expect("with_lock_async should complete");

        assert_eq!(result, "async-result");
        assert!(!lock_path.exists());
    }

    #[test]
    fn should_release_lock_even_if_async_function_throws() {
        let temp_dir = TempDir::new("codegraph-security-test");
        let lock_path = temp_dir.join("test.lock");
        let mut lock = FileLock::new(&lock_path);

        let result = block_on(lock.with_lock_async(|| async { Err::<(), &str>("async error") }))
            .expect("with_lock_async should release after rejected work");

        assert_eq!(result, Err("async error"));
        assert!(!lock_path.exists());
    }

    #[test]
    fn release_should_be_idempotent() {
        let temp_dir = TempDir::new("codegraph-security-test");
        let lock_path = temp_dir.join("test.lock");
        let mut lock = FileLock::new(&lock_path);

        lock.acquire().expect("lock should acquire");
        lock.release();
        lock.release();
    }
}

mod path_traversal_prevention {
    use super::*;

    fn fixture() -> (TempDir, CodeGraph) {
        let test_dir = TempDir::new("codegraph-security-test");
        let src_dir = test_dir.join("src");
        fs::create_dir(&src_dir).expect("failed to create src dir");
        fs::write(
            src_dir.join("hello.ts"),
            "export function hello(): string { return \"hi\"; }\n",
        )
        .expect("failed to write source fixture");

        let mut cg = CodeGraph::init_sync(test_dir.path()).expect("failed to initialize CodeGraph");
        let _ = cg.index_all(IndexOptions::default());
        (test_dir, cg)
    }

    #[test]
    fn should_read_code_for_valid_nodes_within_project() {
        let (_test_dir, mut cg) = fixture();

        let nodes = cg.get_nodes_by_kind(NodeKind::Function);
        let hello = nodes
            .iter()
            .find(|node| node.name == "hello")
            .expect("hello function should be indexed");
        let code = cg.get_code(&hello.id).expect("hello code should be served");

        assert!(code.contains("hello"));
    }

    #[test]
    fn should_return_null_for_non_existent_node() {
        let (_test_dir, mut cg) = fixture();

        assert_eq!(cg.get_code("does-not-exist"), None);
    }
}

mod symlink_escape_prevention_527 {
    use super::*;

    struct Fixture {
        root_dir: TempDir,
        outside_dir: TempDir,
        root: PathBuf,
        outside: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let root_dir = TempDir::new("cg-root");
            let outside_dir = TempDir::new("cg-outside");
            let root = root_dir.realpath();
            let outside = outside_dir.realpath();
            fs::create_dir(root.join("src")).expect("failed to create root src");
            fs::write(root.join("src").join("in.ts"), "export const x = 1;\n")
                .expect("failed to write in-root source");
            fs::create_dir(outside.join("pkg")).expect("failed to create outside pkg");
            fs::write(outside.join("pkg").join("secret.txt"), "TOP-SECRET\n")
                .expect("failed to write outside secret");
            Self {
                root_dir,
                outside_dir,
                root,
                outside,
            }
        }
    }

    #[test]
    fn allows_a_real_file_inside_the_root_and_realpaths_consistently() {
        let fixture = Fixture::new();

        assert!(validate_path_within_root(&fixture.root, "src/in.ts").is_some());
        assert!(fixture.root_dir.path().exists());
        assert!(fixture.outside_dir.path().exists());
    }

    #[test]
    fn allows_a_not_yet_existing_path_inside_the_root_enoent_files_about_to_be_written() {
        let fixture = Fixture::new();

        assert!(validate_path_within_root(&fixture.root, "src/will-write.ts").is_some());
    }

    #[test]
    fn rejects_a_lexical_parent_traversal_out_of_the_root() {
        let fixture = Fixture::new();
        let path = format!(
            "../{}/pkg/secret.txt",
            fixture
                .outside
                .file_name()
                .expect("outside dir should have basename")
                .to_string_lossy()
        );

        assert!(validate_path_within_root(&fixture.root, path).is_none());
    }

    #[test]
    fn rejects_an_in_repo_symlink_to_an_out_of_root_file() {
        let fixture = Fixture::new();
        if !symlink_file(
            &fixture.outside.join("pkg").join("secret.txt"),
            &fixture.root.join("escape"),
        ) {
            return;
        }

        assert!(validate_path_within_root(&fixture.root, "escape").is_none());
    }

    #[test]
    fn rejects_a_path_that_escapes_through_an_in_repo_symlink_to_an_out_of_root_dir() {
        let fixture = Fixture::new();
        if !symlink_dir(
            &fixture.outside.join("pkg"),
            &fixture.root.join("escapedir"),
        ) {
            return;
        }

        assert!(validate_path_within_root(&fixture.root, "escapedir/secret.txt").is_none());
    }

    #[test]
    fn still_allows_an_in_repo_symlink_that_stays_within_the_root_no_over_blocking() {
        let fixture = Fixture::new();
        if !symlink_file(
            &fixture.root.join("src").join("in.ts"),
            &fixture.root.join("src").join("inlink.ts"),
        ) {
            return;
        }

        assert!(validate_path_within_root(&fixture.root, "src/inlink.ts").is_some());
    }

    #[test]
    fn end_to_end_get_code_never_serves_an_out_of_root_file_reached_via_a_dir_symlink() {
        let fixture = Fixture::new();
        fs::write(
            fixture.outside.join("pkg").join("leak.ts"),
            "export function leaked() { return \"LEAKED-ZZZ-9\"; }\n",
        )
        .expect("failed to write leak fixture");
        if !symlink_dir(&fixture.outside.join("pkg"), &fixture.root.join("vendored")) {
            return;
        }

        let mut cg = CodeGraph::init_sync(&fixture.root).expect("failed to initialize CodeGraph");
        let _ = cg.index_all(IndexOptions::default());
        for node in cg.get_nodes_by_kind(NodeKind::Function) {
            let code = cg.get_code(&node.id).unwrap_or_default();
            assert!(!code.contains("LEAKED-ZZZ-9"));
        }
        cg.close();
    }
}

mod validate_project_path_sensitive_directory_blocking {
    use super::*;

    #[test]
    #[cfg_attr(windows, ignore = "process.platform !== 'win32'")]
    fn blocks_posix_system_directories_exact_match() {
        let root = validate_project_path("/").expect("filesystem root should be blocked");
        let etc = validate_project_path("/etc").expect("/etc should be blocked");

        assert!(root.to_lowercase().contains("sensitive system directory"));
        assert!(etc.to_lowercase().contains("sensitive system directory"));
    }

    #[test]
    fn allows_a_normal_existing_directory() {
        let dir = TempDir::new("cg-validate");

        assert_eq!(validate_project_path(dir.path()), None);
    }

    #[test]
    #[cfg_attr(not(windows), ignore = "process.platform === 'win32'")]
    fn blocks_windows_system_directories_regardless_of_case() {
        for path in ["C:\\Windows", "c:\\windows", "C:\\WINDOWS\\System32"] {
            let message =
                validate_project_path(path).expect("Windows system path should be blocked");
            assert!(
                message
                    .to_lowercase()
                    .contains("sensitive system directory"),
                "{message}"
            );
        }
    }
}

mod mcp_input_validation {
    use super::*;

    fn handler() -> ToolHandler {
        ToolHandler::new(true)
    }

    #[test]
    fn should_reject_non_string_query_in_codegraph_search() {
        let mut handler = handler();
        let result = handler.execute(
            "rustcodegraph_search",
            &text_args(&[("query", Value::Null)]),
        );

        assert_eq!(result.is_error, Some(true));
        assert!(first_text(&result).contains("non-empty string"));
    }

    #[test]
    fn should_reject_empty_string_query_in_codegraph_search() {
        let mut handler = handler();
        let result = handler.execute("rustcodegraph_search", &text_args(&[("query", json!(""))]));

        assert_eq!(result.is_error, Some(true));
        assert!(first_text(&result).contains("non-empty string"));
    }

    #[test]
    fn should_accept_valid_query_in_codegraph_search() {
        let mut handler = handler();
        let result = handler.execute(
            "rustcodegraph_search",
            &text_args(&[("query", json!("example"))]),
        );

        assert_not_error(&result);
    }

    #[test]
    fn should_clamp_limit_to_valid_range_in_codegraph_search() {
        let mut handler = handler();
        let result = handler.execute(
            "rustcodegraph_search",
            &text_args(&[("query", json!("example")), ("limit", json!(999999))]),
        );

        assert_not_error(&result);
    }

    #[test]
    fn should_reject_non_string_symbol_in_codegraph_callers() {
        let mut handler = handler();
        let result = handler.execute(
            "rustcodegraph_callers",
            &text_args(&[("symbol", json!(123))]),
        );

        assert_eq!(result.is_error, Some(true));
        assert!(first_text(&result).contains("non-empty string"));
    }

    #[test]
    fn should_reject_non_string_query_in_codegraph_explore() {
        let mut handler = handler();
        let result = handler.execute("rustcodegraph_explore", &Map::new());

        assert_eq!(result.is_error, Some(true));
        assert!(first_text(&result).contains("non-empty string"));
    }

    #[test]
    fn should_truncate_oversized_tool_output() {
        let huge = (0..3000)
            .map(|i| format!("symbol_{i}_{}\n", "x".repeat(40)))
            .collect::<String>();
        let result = truncate_output(&huge);

        assert!(result.contains("... (output truncated)"));
    }

    #[test]
    fn should_reject_non_string_symbol_in_codegraph_impact() {
        let mut handler = handler();
        let result = handler.execute("rustcodegraph_impact", &text_args(&[("symbol", json!([]))]));

        assert_eq!(result.is_error, Some(true));
    }

    #[test]
    fn should_reject_non_string_symbol_in_codegraph_node() {
        let mut handler = handler();
        let result = handler.execute(
            "rustcodegraph_node",
            &text_args(&[("symbol", json!(false))]),
        );

        assert_eq!(result.is_error, Some(true));
    }

    #[test]
    fn should_reject_non_string_symbol_in_codegraph_callees() {
        let mut handler = handler();
        let result = handler.execute(
            "rustcodegraph_callees",
            &text_args(&[("symbol", json!({}))]),
        );

        assert_eq!(result.is_error, Some(true));
    }

    #[test]
    fn should_handle_nan_limit_gracefully() {
        let mut handler = handler();
        let result = handler.execute(
            "rustcodegraph_search",
            &text_args(&[("query", json!("example")), ("limit", json!("abc"))]),
        );

        assert_not_error(&result);
    }

    #[test]
    fn should_handle_negative_limit_gracefully() {
        let mut handler = handler();
        let result = handler.execute(
            "rustcodegraph_search",
            &text_args(&[("query", json!("example")), ("limit", json!(-5))]),
        );

        assert_not_error(&result);
    }

    #[test]
    #[cfg_attr(windows, ignore = "process.platform !== 'win32'")]
    fn rejects_a_sensitive_posix_project_path_etc_via_the_mcp_handler() {
        let mut handler = handler();
        let result = handler.execute(
            "rustcodegraph_search",
            &text_args(&[("query", json!("example")), ("projectPath", json!("/etc"))]),
        );

        assert_eq!(result.is_error, Some(true));
        assert!(
            first_text(&result)
                .to_lowercase()
                .contains("sensitive system directory")
        );
    }

    #[test]
    #[cfg_attr(not(windows), ignore = "process.platform === 'win32'")]
    fn rejects_a_sensitive_windows_project_path_c_windows_via_the_mcp_handler() {
        let mut handler = handler();
        let result = handler.execute(
            "rustcodegraph_search",
            &text_args(&[
                ("query", json!("example")),
                ("projectPath", json!("C:\\Windows")),
            ]),
        );

        assert_eq!(result.is_error, Some(true));
        assert!(
            first_text(&result)
                .to_lowercase()
                .contains("sensitive system directory")
        );
    }
}

mod atomic_writes {
    use super::*;

    #[test]
    fn should_not_leave_temp_files_on_success() {
        let temp_dir = TempDir::new("codegraph-security-test");
        let config_dir = temp_dir.join(".claude");
        fs::create_dir_all(&config_dir).expect("failed to create config dir");
        let test_file = config_dir.join("test.json");
        let tmp_path = test_file.with_file_name(format!(
            "{}.tmp.{}",
            test_file
                .file_name()
                .expect("test file should have filename")
                .to_string_lossy(),
            std::process::id()
        ));

        fs::write(&tmp_path, "{\"test\": true}").expect("failed to write temp file");
        fs::rename(&tmp_path, &test_file).expect("failed to atomically rename temp file");

        assert!(test_file.exists());
        assert!(!tmp_path.exists());

        let content: Value = serde_json::from_str(
            &fs::read_to_string(&test_file).expect("test file should be readable"),
        )
        .expect("test file should be JSON");
        assert_eq!(content["test"], true);
    }
}

mod source_file_detection_is_source_file {
    use super::*;

    #[test]
    fn selects_files_by_supported_extension() {
        assert!(is_source_file("src/index.ts"));
        assert!(is_source_file("src/deep/nested/file.ts"));
        assert!(is_source_file("src/component.tsx"));
        assert!(is_source_file("lib/util.js"));
        assert!(is_source_file("src/main.py"));
    }

    #[test]
    fn rejects_unsupported_extensions_and_extensionless_files() {
        assert!(!is_source_file("src/component.css"));
        assert!(!is_source_file("README.md"));
        assert!(!is_source_file("Makefile"));
        assert!(!is_source_file(".gitignore"));
    }

    #[test]
    fn matches_regardless_of_leading_dot_directories() {
        assert!(is_source_file(".hidden/index.ts"));
    }
}

mod json_parse_error_boundaries_in_db {
    use super::*;

    #[derive(Default)]
    struct FakeDb {
        row: Option<SqliteRow>,
        open: bool,
    }

    impl FakeDb {
        fn with_row(row: SqliteRow) -> Self {
            Self {
                row: Some(row),
                open: true,
            }
        }
    }

    struct FakeStatement {
        row: Option<SqliteRow>,
    }

    impl SqliteStatement for FakeStatement {
        fn run(&mut self, _params: SqliteParams) -> SqliteResult<RunResult> {
            Ok(RunResult {
                changes: 1,
                last_insert_rowid: 1,
            })
        }

        fn get(&mut self, _params: SqliteParams) -> SqliteResult<Option<SqliteRow>> {
            Ok(self.row.clone())
        }

        fn all(&mut self, _params: SqliteParams) -> SqliteResult<Vec<SqliteRow>> {
            Ok(self.row.clone().into_iter().collect())
        }

        fn iterate<'stmt>(
            &'stmt mut self,
            _params: SqliteParams,
        ) -> SqliteResult<Box<dyn Iterator<Item = SqliteResult<SqliteRow>> + 'stmt>> {
            Ok(Box::new(self.row.clone().into_iter().map(Ok)))
        }
    }

    impl SqliteDatabase for FakeDb {
        fn prepare(&mut self, _sql: &str) -> SqliteResult<Box<dyn SqliteStatement>> {
            Ok(Box::new(FakeStatement {
                row: self.row.clone(),
            }))
        }

        fn exec(&mut self, _sql: &str) -> SqliteResult<()> {
            Ok(())
        }

        fn pragma(
            &mut self,
            _pragma: &str,
            _options: Option<PragmaOptions>,
        ) -> SqliteResult<Option<SqliteValue>> {
            Ok(None)
        }

        fn transaction(
            &mut self,
            f: &mut dyn FnMut(&mut dyn SqliteDatabase) -> SqliteResult<()>,
        ) -> SqliteResult<()> {
            f(self)
        }

        fn close(&mut self) -> SqliteResult<()> {
            self.open = false;
            Ok(())
        }

        fn is_open(&self) -> bool {
            self.open
        }
    }

    fn row(pairs: Vec<(&str, SqliteValue)>) -> SqliteRow {
        pairs
            .into_iter()
            .map(|(key, value)| (key.to_string(), value))
            .collect::<HashMap<_, _>>()
    }

    fn now_ms() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after Unix epoch")
            .as_millis() as i64
    }

    #[test]
    fn should_not_crash_when_node_has_malformed_json_in_decorators_column() {
        let node_row = row(vec![
            ("id", "test-node-1".into()),
            ("kind", "function".into()),
            ("name", "myFunc".into()),
            ("qualified_name", "myFunc".into()),
            ("file_path", "test.ts".into()),
            ("language", "typescript".into()),
            ("start_line", 1.into()),
            ("end_line", 5.into()),
            ("start_column", 0.into()),
            ("end_column", 0.into()),
            ("decorators", "{not valid json!!!}".into()),
            ("is_exported", 0.into()),
            ("is_async", 0.into()),
            ("is_static", 0.into()),
            ("is_abstract", 0.into()),
            ("updated_at", now_ms().into()),
        ]);
        let mut db = FakeDb::with_row(node_row);
        let mut queries = QueryBuilder::new(&mut db);

        let node = queries
            .get_node_by_id("test-node-1")
            .expect("malformed decorators should not crash")
            .expect("node should be returned");

        assert_eq!(node.name, "myFunc");
        assert_eq!(node.decorators, None);
    }

    #[test]
    fn should_not_crash_when_edge_has_malformed_json_in_metadata_column() {
        let edge_row = row(vec![
            ("source", "node-a".into()),
            ("target", "node-b".into()),
            ("kind", "calls".into()),
            ("metadata", "broken json {{{".into()),
        ]);
        let mut db = FakeDb::with_row(edge_row);
        let mut queries = QueryBuilder::new(&mut db);

        let edges = queries
            .get_outgoing_edges("node-a", None, None)
            .expect("malformed metadata should not crash");

        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].source, "node-a");
        assert_eq!(edges[0].target, "node-b");
        assert_eq!(edges[0].metadata, None);
    }

    #[test]
    fn should_not_crash_when_file_record_has_malformed_json_in_errors_column() {
        let file_row = row(vec![
            ("path", "test.ts".into()),
            ("content_hash", "abc123".into()),
            ("language", "typescript".into()),
            ("size", 100.into()),
            ("modified_at", now_ms().into()),
            ("indexed_at", now_ms().into()),
            ("node_count", 5.into()),
            ("errors", "not-an-array".into()),
        ]);
        let mut db = FakeDb::with_row(file_row);
        let mut queries = QueryBuilder::new(&mut db);

        let file = queries
            .get_file_by_path("test.ts")
            .expect("malformed errors should not crash")
            .expect("file should be returned");

        assert_eq!(file.path, "test.ts");
        assert!(file.errors.is_none());
    }
}

mod symlink_cycle_detection {
    use super::*;

    #[test]
    fn should_handle_symlink_cycle_without_infinite_loop() {
        let temp_dir = TempDir::new("codegraph-security-test");
        let src_dir = temp_dir.join("src");
        fs::create_dir(&src_dir).expect("failed to create src dir");
        fs::write(src_dir.join("index.ts"), "export const x = 1;\n")
            .expect("failed to write index fixture");

        if !symlink_dir(temp_dir.path(), &src_dir.join("loop")) {
            return;
        }

        let files = scan_directory(temp_dir.path(), None);

        assert!(files.contains(&"src/index.ts".to_string()), "{files:?}");
        let index_files = files
            .iter()
            .filter(|file| file.ends_with("index.ts"))
            .collect::<Vec<_>>();
        assert_eq!(index_files.len(), 1, "{files:?}");
    }

    #[test]
    fn should_follow_valid_symlinks_to_directories() {
        let temp_dir = TempDir::new("codegraph-security-test");
        let real_dir = temp_dir.join("real");
        fs::create_dir(&real_dir).expect("failed to create real dir");
        fs::write(real_dir.join("hello.ts"), "export function hello() {}\n")
            .expect("failed to write hello fixture");
        let src_dir = temp_dir.join("src");
        fs::create_dir(&src_dir).expect("failed to create src dir");
        if !symlink_dir(&real_dir, &src_dir.join("linked")) {
            return;
        }

        let files = scan_directory(temp_dir.path(), None);

        assert!(
            files.iter().any(|file| file.contains("hello.ts")),
            "{files:?}"
        );
    }

    #[test]
    fn should_skip_broken_symlinks_gracefully() {
        let temp_dir = TempDir::new("codegraph-security-test");
        let src_dir = temp_dir.join("src");
        fs::create_dir(&src_dir).expect("failed to create src dir");
        fs::write(src_dir.join("valid.ts"), "export const y = 2;\n")
            .expect("failed to write valid fixture");

        if !symlink_dir(Path::new("/nonexistent/path"), &src_dir.join("broken")) {
            return;
        }

        let files = scan_directory(temp_dir.path(), None);

        assert!(files.contains(&"src/valid.ts".to_string()), "{files:?}");
    }
}

#[test]
fn ignored_case_blocker_is_recorded_for_this_port() {
    assert_eq!(
        BACKEND_BLOCKER,
        "Rust CodeGraph indexing/query backend is not wired yet"
    );
}
