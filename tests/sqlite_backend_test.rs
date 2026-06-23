//! SQLite backend reporting.
//!
//! Pin that DatabaseConnection / CodeGraph report the Rust SQLite backend and
//! come up in WAL.
//!
//! This is the Rust port of `__tests__/sqlite-backend.test.ts`.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use rustcodegraph::db::index::DatabaseConnection;
use rustcodegraph::{CodeGraph, InitOptions};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

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
            let path = std::env::temp_dir().join(format!(
                "{prefix}-{}-{unique}-{counter}",
                std::process::id()
            ));
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
}

impl Drop for TempDir {
    fn drop(&mut self) {
        if self.path.exists() {
            let _ = fs::remove_dir_all(&self.path);
        }
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
}

impl Drop for CodeGraphGuard {
    fn drop(&mut self) {
        if let Some(mut value) = self.value.take() {
            value.destroy();
        }
    }
}

mod database_connection_backend_reporting {
    use super::*;

    #[test]
    fn reports_the_sqlite_backend_in_wal_for_an_initialized_db() {
        let dir = TempDir::new("codegraph-backend");
        let mut conn = DatabaseConnection::initialize(dir.path().join("test.db"))
            .expect("database should initialize");

        assert_eq!(conn.get_backend().as_str(), "node-sqlite");
        assert_eq!(
            conn.get_journal_mode()
                .expect("journal mode should be readable"),
            "wal"
        );

        conn.close().expect("database should close");
    }

    #[test]
    fn code_graph_get_backend_delegates_to_the_underlying_database_connection() {
        let dir = TempDir::new("codegraph-backend");
        fs::write(dir.path().join("x.ts"), "export function x(): void {}\n")
            .expect("fixture file should be written");

        let cg = CodeGraphGuard::new(
            CodeGraph::init(dir.path(), InitOptions { index: true })
                .expect("CodeGraph should initialize"),
        );

        assert_eq!(cg.get().get_backend(), "node-sqlite");
    }
}
