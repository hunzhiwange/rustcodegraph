//! SQLite backend real-index regression coverage (issue #238 follow-up).
//!
//! The Rust SQLite backend drives a real index + queries here, so WAL, FTS5
//! search, and named-param writes are all exercised end-to-end.
//!
//! This is the Rust port of `__tests__/node-sqlite-backend.test.ts`.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use rustcodegraph::{CodeGraph, InitOptions};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn sqlite_backend_available() -> bool {
    true
}

struct TempProject {
    path: PathBuf,
}

impl TempProject {
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
                Err(err) => {
                    panic!("failed to create temp project {}: {err}", path.display());
                }
            }
        }
        panic!("failed to create unique temp project with prefix {prefix}");
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn write(&self, relative_path: &str, content: &str) {
        fs::write(self.path.join(relative_path), content)
            .unwrap_or_else(|err| panic!("failed to write fixture {relative_path}: {err}"));
    }
}

impl Drop for TempProject {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
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

    fn get_mut(&mut self) -> &mut CodeGraph {
        self.value.as_mut().expect("CodeGraph should be present")
    }
}

impl Drop for CodeGraphGuard {
    fn drop(&mut self) {
        if let Some(mut value) = self.value.take() {
            value.close();
        }
    }
}

struct Fixture {
    _temp: TempProject,
    cg: CodeGraphGuard,
}

impl Fixture {
    fn new() -> Self {
        let temp = TempProject::new("cg-nodesqlite");
        temp.write("a.ts", "export function helper(): number { return 1; }\n");
        temp.write(
            "b.ts",
            "import { helper } from './a';\nexport function main(): number { return helper(); }\n",
        );
        let cg = CodeGraph::init(temp.path(), InitOptions { index: true })
            .expect("CodeGraph should initialize");
        Self {
            _temp: temp,
            cg: CodeGraphGuard::new(cg),
        }
    }
}

fn fixture_or_skip() -> Option<Fixture> {
    sqlite_backend_available().then(Fixture::new)
}

mod sqlite_backend_real_index_queries {
    use super::*;

    #[test]
    fn uses_the_sqlite_backend() {
        let Some(fixture) = fixture_or_skip() else {
            return;
        };

        assert_eq!(fixture.cg.get().get_backend(), "node-sqlite");
    }

    #[test]
    fn runs_in_wal_mode_the_whole_reason_it_beats_the_wasm_fallback() {
        let Some(mut fixture) = fixture_or_skip() else {
            return;
        };

        assert_eq!(fixture.cg.get_mut().get_journal_mode(), "wal");
    }

    #[test]
    fn indexed_the_project_write_path_named_param_inserts_via_sqlite() {
        let Some(fixture) = fixture_or_skip() else {
            return;
        };

        let stats = fixture.cg.get().get_stats();
        assert_eq!(stats.file_count, 2);
        assert!(
            stats.node_count > 0,
            "expected indexed nodes, got {stats:?}"
        );
    }

    #[test]
    fn fts5_search_returns_the_indexed_symbol_read_path() {
        let Some(mut fixture) = fixture_or_skip() else {
            return;
        };

        let results = fixture.cg.get_mut().search_nodes("helper", None);
        let names = results
            .into_iter()
            .map(|result| result.node.name)
            .collect::<Vec<_>>();
        assert!(
            names.iter().any(|name| name == "helper"),
            "expected {names:?} to contain helper"
        );
    }

    #[test]
    fn graph_traversal_resolves_the_cross_file_caller() {
        let Some(mut fixture) = fixture_or_skip() else {
            return;
        };

        let helper = fixture
            .cg
            .get_mut()
            .search_nodes("helper", None)
            .into_iter()
            .find(|result| result.node.name == "helper")
            .expect("helper should be indexed");
        let callers = fixture
            .cg
            .get_mut()
            .get_callers(&helper.node.id, 1)
            .into_iter()
            .map(|caller| caller.node.name)
            .collect::<Vec<_>>();
        assert!(
            callers.iter().any(|name| name == "main"),
            "expected {callers:?} to contain main"
        );
    }
}
