//! Issue #238: concurrent locking regressions.
//!
//! This is the Rust port of `__tests__/concurrent-locking.test.ts`.
//!
//! With Rust SQLite WAL as the backend, the fixes that remain relevant:
//! 1. busy_timeout is a bounded few-second wait (not a 2-minute hang) and WAL is
//!    active, so a reader never blocks on a concurrent writer.
//! 2. The MCP ToolHandler resolves a tool-provided `projectPath` for the default
//!    project back to the default project instead of treating it as an unrelated
//!    unindexed workspace.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use rustcodegraph::db::index::DatabaseConnection;
use rustcodegraph::db::sqlite_adapter::{PragmaOptions, SqliteParams, SqliteValue};
use rustcodegraph::mcp::tools::{ToolHandler, ToolResult};
use rustcodegraph::{CodeGraph, InitOptions};
use serde_json::{Map, Value, json};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Normalize a PRAGMA read across return shapes (array | object | scalar).
fn pragma_value(raw: Option<SqliteValue>, _key: &str) -> SqliteValue {
    raw.expect("PRAGMA should return a value")
}

fn pragma_i64(conn: &mut DatabaseConnection, name: &str, key: &str) -> i64 {
    match pragma_value(
        conn.get_db()
            .pragma(name, Some(PragmaOptions { simple: false }))
            .unwrap_or_else(|err| panic!("PRAGMA {name} should be readable: {err}")),
        key,
    ) {
        SqliteValue::Integer(value) => value,
        SqliteValue::Text(value) => value
            .parse::<i64>()
            .unwrap_or_else(|err| panic!("PRAGMA {name} value should parse as i64: {err}")),
        other => panic!("PRAGMA {name} returned unexpected value {other:?}"),
    }
}

fn first_text(result: &ToolResult) -> &str {
    result
        .content
        .first()
        .map(|content| content.text.as_str())
        .unwrap_or("")
}

fn args(items: &[(&str, Value)]) -> Map<String, Value> {
    items
        .iter()
        .map(|(key, value)| ((*key).to_owned(), value.clone()))
        .collect()
}

fn assert_tool_ok(result: &ToolResult) {
    assert_ne!(
        result.is_error,
        Some(true),
        "tool should not fail: {result:?}"
    );
    assert!(
        !first_text(result)
            .to_ascii_lowercase()
            .contains("database is locked"),
        "tool result should not report a lock: {result:?}"
    );
}

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(prefix: &str) -> Self {
        for _ in 0..100 {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time should be after the Unix epoch")
                .as_nanos();
            let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir()
                .join(format!("{prefix}{}-{unique}-{counter}", std::process::id()));
            match fs::create_dir(&path) {
                Ok(()) => return Self { path },
                Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(err) => panic!("failed to create temp dir {}: {err}", path.display()),
            }
        }
        panic!("failed to create unique temp dir with prefix {prefix}");
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn write(&self, relative_path: &str, content: &str) {
        fs::write(self.path.join(relative_path), content)
            .unwrap_or_else(|err| panic!("failed to write fixture {relative_path}: {err}"));
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

struct DbGuard {
    conn: Option<DatabaseConnection>,
}

impl DbGuard {
    fn new(conn: DatabaseConnection) -> Self {
        Self { conn: Some(conn) }
    }

    fn get_mut(&mut self) -> &mut DatabaseConnection {
        self.conn
            .as_mut()
            .expect("database connection should be present")
    }

    fn close(&mut self) {
        if let Some(mut conn) = self.conn.take() {
            let _ = conn.close();
        }
    }
}

impl Drop for DbGuard {
    fn drop(&mut self) {
        self.close();
    }
}

struct CodeGraphGuard {
    value: Option<CodeGraph>,
}

impl CodeGraphGuard {
    fn new(value: CodeGraph) -> Self {
        Self { value: Some(value) }
    }

    fn get(&self) -> &CodeGraph {
        self.value.as_ref().expect("CodeGraph should be present")
    }

    fn close(&mut self) {
        if let Some(mut value) = self.value.take() {
            value.close();
        }
    }
}

impl Drop for CodeGraphGuard {
    fn drop(&mut self) {
        self.close();
    }
}

struct PragmaFixture {
    _dir: TempDir,
    conn: DbGuard,
}

impl PragmaFixture {
    fn new() -> Self {
        let dir = TempDir::new("cg238-pragma-");
        let conn = DatabaseConnection::initialize(dir.path().join("rustcodegraph.db"))
            .expect("database should initialize");
        Self {
            _dir: dir,
            conn: DbGuard::new(conn),
        }
    }
}

struct ToolFixture {
    _dir: TempDir,
    cg: CodeGraphGuard,
    root: String,
    handler: ToolHandler,
}

impl ToolFixture {
    fn new() -> Self {
        let dir = TempDir::new("cg238-tools-");
        dir.write("a.ts", "export function helper(): number { return 1; }\n");
        dir.write(
            "b.ts",
            "import { helper } from './a';\nexport function main(): number { return helper(); }\n",
        );
        let cg = CodeGraph::init(dir.path(), InitOptions { index: true })
            .expect("CodeGraph should initialize");
        let root = cg.get_project_root();
        let mut handler = ToolHandler::new(true);
        handler.set_default_code_graph(&cg);

        Self {
            _dir: dir,
            cg: CodeGraphGuard::new(cg),
            root,
            handler,
        }
    }
}

impl Drop for ToolFixture {
    fn drop(&mut self) {
        self.handler.close_all();
        self.cg.close();
    }
}

mod issue_238_connection_pragmas_1 {
    use super::*;

    #[test]
    fn uses_a_bounded_busy_timeout_not_the_old_2_minute_hang() {
        let mut fixture = PragmaFixture::new();

        let ms = pragma_i64(fixture.conn.get_mut(), "busy_timeout", "timeout");

        assert!(ms > 0, "busy_timeout should be positive, got {ms}");
        assert!(
            ms <= 30_000,
            "busy_timeout should stay far below the old 120000ms hang, got {ms}"
        );
    }

    #[test]
    fn runs_in_wal_mode_the_mode_that_lets_readers_proceed_during_a_write() {
        let mut fixture = PragmaFixture::new();

        let mode = fixture
            .conn
            .get_mut()
            .get_journal_mode()
            .expect("journal mode should be readable");

        assert_eq!(mode, "wal");
    }

    #[test]
    fn get_journal_mode_surfaces_the_effective_mode_for_status_triage() {
        let mut fixture = PragmaFixture::new();

        assert_eq!(
            fixture
                .conn
                .get_mut()
                .get_journal_mode()
                .expect("journal mode should be readable"),
            "wal"
        );
    }
}

mod issue_238_wal_lets_a_reader_proceed_during_a_writer {
    use super::*;

    #[test]
    fn a_read_on_a_2nd_connection_succeeds_while_a_writer_holds_the_lock() {
        let dir = TempDir::new("cg238-wal-");
        let db_path = dir.path().join("rustcodegraph.db");
        let mut writer = DbGuard::new(
            DatabaseConnection::initialize(&db_path).expect("writer database should initialize"),
        );

        // The property only holds under WAL; skip if the filesystem couldn't enable it.
        if writer
            .get_mut()
            .get_journal_mode()
            .expect("journal mode should be readable")
            != "wal"
        {
            return;
        }

        let mut reader =
            DbGuard::new(DatabaseConnection::open(&db_path).expect("reader database should open"));

        writer
            .get_mut()
            .get_db()
            .prepare("BEGIN EXCLUSIVE")
            .expect("BEGIN EXCLUSIVE should prepare")
            .run(SqliteParams::none())
            .expect("writer transaction should begin");

        let t0 = Instant::now();
        let row = reader
            .get_mut()
            .get_db()
            .prepare("SELECT COUNT(*) AS c FROM nodes")
            .expect("reader query should prepare")
            .get(SqliteParams::none())
            .expect("reader query should run")
            .expect("reader query should return a row");
        let waited = t0.elapsed();
        let count = row
            .get("c")
            .and_then(SqliteValue::as_i64)
            .expect("COUNT(*) should be numeric");

        let _ = writer
            .get_mut()
            .get_db()
            .prepare("COMMIT")
            .and_then(|mut stmt| stmt.run(SqliteParams::none()).map(|_| ()));

        assert_eq!(count, 0);
        assert!(
            waited < Duration::from_secs(1),
            "reader should proceed immediately, waited {waited:?}"
        );
    }
}

mod issue_238_tool_handler_reuses_the_default_instance_2 {
    use super::*;

    #[test]
    fn get_code_graph_default_root_returns_the_default_instance_not_a_new_connection() {
        let mut fixture = ToolFixture::new();
        let nested = Path::new(&fixture.root)
            .join("does")
            .join("not")
            .join("exist");

        let resolved = fixture.handler.execute(
            "rustcodegraph_search",
            &args(&[
                ("query", json!("helper")),
                ("projectPath", json!(fixture.root.clone())),
            ]),
        );
        let nested = fixture.handler.execute(
            "rustcodegraph_search",
            &args(&[
                ("query", json!("helper")),
                ("projectPath", json!(nested.to_string_lossy().to_string())),
            ]),
        );

        assert_tool_ok(&resolved);
        assert_tool_ok(&nested);
        assert!(first_text(&resolved).contains("helper"), "{resolved:?}");
        assert!(first_text(&nested).contains("helper"), "{nested:?}");
        assert!(fixture.handler.has_default_code_graph());
        assert_eq!(fixture.cg.get().get_project_root(), fixture.root);
    }

    #[test]
    fn concurrent_read_tool_calls_mixed_project_path_all_succeed_without_database_is_locked() {
        let fixture = ToolFixture::new();
        let root = fixture.root.clone();

        let calls = vec![
            ("rustcodegraph_search", args(&[("query", json!("helper"))])),
            (
                "rustcodegraph_search",
                args(&[
                    ("query", json!("helper")),
                    ("projectPath", json!(root.clone())),
                ]),
            ),
            (
                "rustcodegraph_callers",
                args(&[
                    ("symbol", json!("helper")),
                    ("projectPath", json!(root.clone())),
                ]),
            ),
            ("rustcodegraph_callees", args(&[("symbol", json!("main"))])),
            (
                "rustcodegraph_files",
                args(&[("projectPath", json!(root.clone()))]),
            ),
            (
                "rustcodegraph_status",
                args(&[("projectPath", json!(root.clone()))]),
            ),
        ];

        let handles = calls
            .into_iter()
            .map(|(tool, args)| {
                let mut handler = fixture.handler.clone();
                let tool = tool.to_owned();
                thread::spawn(move || handler.execute(&tool, &args))
            })
            .collect::<Vec<_>>();

        let results = handles
            .into_iter()
            .map(|handle| handle.join().expect("tool thread should not panic"))
            .collect::<Vec<_>>();

        for result in &results {
            assert_tool_ok(result);
        }
    }
}
