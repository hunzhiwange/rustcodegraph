//! PR #19 improvement regressions.
//!
//! This is the Rust port of `__tests__/pr19-improvements.test.ts`. Cases that
//! depended on Node's `skipIf(!HAS_SQLITE)` run against bundled rusqlite when
//! possible. Cases that depend on still-deferred Rust parser, graph traversal,
//! or deeper MCP wiring are preserved as ignored tests with the same case names.

use std::collections::HashMap;
use std::fs;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Once;
use std::sync::atomic::{AtomicU64, Ordering};
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};
use std::time::{SystemTime, UNIX_EPOCH};

use rustcodegraph::db::index::DatabaseConnection;
use rustcodegraph::db::migrations::{CURRENT_SCHEMA_VERSION, get_pending_migrations, migrations};
use rustcodegraph::db::queries::QueryBuilder;
use rustcodegraph::db::sqlite_adapter::{PragmaOptions, SqliteValue};
use rustcodegraph::extraction::grammars::{
    clear_parser_cache, get_parser, get_supported_languages, get_unavailable_grammar_errors,
    init_grammars, is_language_supported, load_all_grammars,
};
use rustcodegraph::extraction::tree_sitter::extract_from_source;
use rustcodegraph::mcp::tools::{ToolHandler, truncate_output};
use rustcodegraph::resolution::index::ReferenceResolver;
use rustcodegraph::resolution::types::UnresolvedRef;
use rustcodegraph::types::{
    Language, Node, NodeKind, ReferenceKind, TraversalDirection, TraversalOptions,
    UnresolvedReference,
};
use rustcodegraph::{CodeGraph, IndexOptions};
use serde_json::Value;

static GRAMMAR_INIT: Once = Once::new();
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn noop_raw_waker() -> RawWaker {
    fn clone(_: *const ()) -> RawWaker {
        noop_raw_waker()
    }
    fn wake(_: *const ()) {}
    fn wake_by_ref(_: *const ()) {}
    fn drop(_: *const ()) {}

    static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, wake, wake_by_ref, drop);
    RawWaker::new(std::ptr::null(), &VTABLE)
}

fn block_on<F: Future>(future: F) -> F::Output {
    let waker = unsafe { Waker::from_raw(noop_raw_waker()) };
    let mut cx = Context::from_waker(&waker);
    let mut future = Pin::from(Box::new(future));

    loop {
        match future.as_mut().poll(&mut cx) {
            Poll::Ready(value) => return value,
            Poll::Pending => std::thread::yield_now(),
        }
    }
}

fn before_all_init_grammars() {
    GRAMMAR_INIT.call_once(|| {
        let _ = block_on(init_grammars());
        let _ = block_on(load_all_grammars());
    });
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after the Unix epoch")
        .as_millis() as i64
}

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(prefix: &str) -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after the Unix epoch")
            .as_nanos();
        let suffix = TEMP_COUNTER.fetch_add(1, Ordering::SeqCst);
        let path =
            std::env::temp_dir().join(format!("{prefix}-{}-{unique}-{suffix}", std::process::id()));
        fs::create_dir_all(&path)
            .unwrap_or_else(|err| panic!("failed to create temp dir {}: {err}", path.display()));
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        cleanup_temp_dir(&self.path);
    }
}

fn create_temp_dir() -> TempDir {
    TempDir::new("codegraph-pr19-test")
}

fn cleanup_temp_dir(dir: &Path) {
    if dir.exists() {
        let _ = fs::remove_dir_all(dir);
    }
}

fn has_sqlite_bindings() -> bool {
    rusqlite::Connection::open_in_memory()
        .and_then(|conn| conn.close().map_err(|(_, err)| err))
        .is_ok()
}

fn sqlite_unavailable() -> bool {
    !has_sqlite_bindings()
}

fn test_node(id: &str, name: &str, start_line: u64) -> Node {
    Node {
        id: id.to_owned(),
        kind: NodeKind::Function,
        name: name.to_owned(),
        qualified_name: format!("test::{name}"),
        file_path: "test.ts".to_owned(),
        language: Language::TypeScript,
        start_line,
        end_line: start_line + 4,
        start_column: 0,
        end_column: 1,
        docstring: None,
        signature: None,
        visibility: None,
        is_exported: Some(false),
        is_async: Some(false),
        is_static: Some(false),
        is_abstract: Some(false),
        decorators: None,
        type_parameters: None,
        return_type: None,
        updated_at: now_ms(),
    }
}

fn unresolved_call(from_node_id: &str, reference_name: &str, line: u64) -> UnresolvedReference {
    UnresolvedReference {
        from_node_id: from_node_id.to_owned(),
        reference_name: reference_name.to_owned(),
        reference_kind: ReferenceKind::Calls,
        line,
        column: 4,
        file_path: Some("test.ts".to_owned()),
        language: Some(Language::TypeScript),
        candidates: None,
    }
}

fn pragma_i64(db: &mut DatabaseConnection, name: &str) -> i64 {
    match db
        .get_db()
        .pragma(name, Some(PragmaOptions { simple: true }))
        .unwrap_or_else(|err| panic!("PRAGMA {name} should be readable: {err}"))
        .unwrap_or_else(|| panic!("PRAGMA {name} should return a value"))
    {
        SqliteValue::Integer(value) => value,
        SqliteValue::Text(value) => value
            .parse::<i64>()
            .unwrap_or_else(|err| panic!("PRAGMA {name} value should parse as i64: {err}")),
        other => panic!("PRAGMA {name} returned unexpected value {other:?}"),
    }
}

fn write_fixture(path: impl AsRef<Path>, source: &str) {
    fs::write(path.as_ref(), source)
        .unwrap_or_else(|err| panic!("failed to write {}: {err}", path.as_ref().display()));
}

fn find_symbol_matches(cg: &mut CodeGraph, symbol: &str) -> Vec<Node> {
    let exact = cg.get_nodes_by_name(symbol);
    if !exact.is_empty() {
        return exact;
    }

    cg.search_nodes(symbol, None)
        .into_iter()
        .next()
        .map(|result| vec![result.node])
        .unwrap_or_default()
}

include!("pr19_improvements_test/lazy_grammar_loading.rs");

include!("pr19_improvements_test/arrow_function_body_traversal.rs");

include!("pr19_improvements_test/native_indexing_runtime.rs");

include!("pr19_improvements_test/graph_traversal_both_direction.rs");

include!("pr19_improvements_test/best_candidate_resolution.rs");

include!("pr19_improvements_test/schema_v2_migration.rs");

include!("pr19_improvements_test/database_layer_improvements.rs");

include!("pr19_improvements_test/resolution_warm_caches.rs");

include!("pr19_improvements_test/mcp_tool_improvements.rs");

include!("pr19_improvements_test/cli_uninit.rs");

include!("pr19_improvements_test/tree_sitter_rust_setup.rs");

include!("pr19_improvements_test/float32array_fix.rs");
