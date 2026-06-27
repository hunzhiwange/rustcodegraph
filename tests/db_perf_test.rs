//! DB performance / correctness tests.
//!
//! Regression tests for:
//! 1. Batch `get_nodes_by_ids` collapses graph-traversal N+1 reads.
//! 2. `insert_node` invalidates the LRU cache so INSERT OR REPLACE does not
//!    serve a stale cached row on next `get_node_by_id`.
//! 3. `run_maintenance` runs best-effort PRAGMAs without throwing.
//! 4. `insert_edges` validates endpoints from the DB, not stale node cache.
//!
//! This is the Rust port of `__tests__/db-perf.test.ts`.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use rustcodegraph::db::index::DatabaseConnection;
use rustcodegraph::db::queries::QueryBuilder;
use rustcodegraph::types::{Edge, EdgeKind, Language, Node, NodeKind};

static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);

fn test_temp_root() -> PathBuf {
    std::env::var_os("CARGO_TARGET_TMPDIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
}

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(prefix: &str) -> Self {
        let root = test_temp_root();
        fs::create_dir_all(&root).unwrap_or_else(|err| {
            panic!("failed to create test temp root {}: {err}", root.display())
        });
        for _ in 0..100 {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time should be after the Unix epoch")
                .as_nanos();
            let suffix = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
            let path = root.join(format!("{prefix}-{}-{unique}-{suffix}", std::process::id()));
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

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after the Unix epoch")
        .as_millis() as i64
}

fn setup(prefix: &str) -> (TempDir, DatabaseConnection, PathBuf) {
    let dir = TempDir::new(prefix);
    let db_path = dir.path().join("test.db");
    let db = DatabaseConnection::initialize(&db_path).expect("database should initialize");
    (dir, db, db_path)
}

fn make_node(id: &str) -> Node {
    make_node_named(id, id)
}

fn make_node_named(id: &str, name: &str) -> Node {
    Node {
        id: id.to_owned(),
        kind: NodeKind::Function,
        name: name.to_owned(),
        qualified_name: name.to_owned(),
        file_path: "a.ts".to_owned(),
        language: Language::TypeScript,
        start_line: 1,
        end_line: 1,
        start_column: 0,
        end_column: 0,
        docstring: None,
        signature: None,
        visibility: None,
        is_exported: None,
        is_async: None,
        is_static: None,
        is_abstract: None,
        decorators: None,
        type_parameters: None,
        return_type: None,
        updated_at: now_ms(),
    }
}

fn edge(source: &str, target: &str, kind: EdgeKind) -> Edge {
    Edge {
        source: source.to_owned(),
        target: target.to_owned(),
        kind,
        metadata: None,
        line: None,
        column: None,
        provenance: None,
    }
}

fn update_node_name(db_path: &Path, id: &str, name: &str) {
    let connection = rusqlite::Connection::open(db_path)
        .unwrap_or_else(|err| panic!("raw sqlite connection should open: {err}"));
    connection
        .execute(
            "UPDATE nodes SET name = ?1 WHERE id = ?2",
            rusqlite::params![name, id],
        )
        .unwrap_or_else(|err| panic!("node row should update: {err}"));
}

fn delete_node_row(db_path: &Path, id: &str) {
    let connection = rusqlite::Connection::open(db_path)
        .unwrap_or_else(|err| panic!("raw sqlite connection should open: {err}"));
    connection
        .execute("DELETE FROM nodes WHERE id = ?1", rusqlite::params![id])
        .unwrap_or_else(|err| panic!("node row should delete: {err}"));
}

mod get_nodes_by_ids_batch_lookup {
    use super::*;

    #[test]
    fn returns_a_map_keyed_by_id_with_one_entry_per_existing_node() {
        let (_dir, mut db, _db_path) = setup("db-perf-batch");

        {
            let mut q = QueryBuilder::new(db.get_db());
            q.insert_nodes(&[make_node("n1"), make_node("n2"), make_node("n3")])
                .expect("nodes should insert");

            let ids = vec!["n1".to_owned(), "n2".to_owned(), "n3".to_owned()];
            let out = q.get_nodes_by_ids(&ids).expect("nodes should load");

            assert_eq!(out.len(), 3);
            assert_eq!(out.get("n1").expect("n1 should exist").name, "n1");
            assert_eq!(out.get("n3").expect("n3 should exist").name, "n3");
        }

        db.close().expect("database should close");
    }

    #[test]
    fn omits_missing_ids_from_the_result_map_no_nulls_no_exceptions() {
        let (_dir, mut db, _db_path) = setup("db-perf-batch");

        {
            let mut q = QueryBuilder::new(db.get_db());
            q.insert_nodes(&[make_node("n1"), make_node("n2")])
                .expect("nodes should insert");

            let ids = vec!["n1".to_owned(), "missing".to_owned(), "n2".to_owned()];
            let out = q.get_nodes_by_ids(&ids).expect("nodes should load");

            assert_eq!(out.len(), 2);
            assert!(!out.contains_key("missing"));
            assert!(out.contains_key("n1"));
            assert!(out.contains_key("n2"));
        }

        db.close().expect("database should close");
    }

    #[test]
    fn handles_an_empty_input_array() {
        let (_dir, mut db, _db_path) = setup("db-perf-batch");

        {
            let mut q = QueryBuilder::new(db.get_db());
            let ids = Vec::<String>::new();
            let out = q.get_nodes_by_ids(&ids).expect("empty batch should load");
            assert_eq!(out.len(), 0);
        }

        db.close().expect("database should close");
    }

    #[test]
    fn handles_batches_over_the_sqlite_parameter_limit_chunking() {
        let (_dir, mut db, _db_path) = setup("db-perf-batch");

        {
            let mut q = QueryBuilder::new(db.get_db());
            let nodes = (0..1500)
                .map(|i| make_node(&format!("n{i}")))
                .collect::<Vec<_>>();
            q.insert_nodes(&nodes).expect("nodes should insert");

            let ids = nodes.iter().map(|node| node.id.clone()).collect::<Vec<_>>();
            let out = q.get_nodes_by_ids(&ids).expect("nodes should load");

            assert_eq!(out.len(), 1500);
            assert!(out.contains_key("n0"));
            assert!(out.contains_key("n750"));
            assert!(out.contains_key("n1499"));
        }

        db.close().expect("database should close");
    }

    #[test]
    fn serves_cache_hits_from_memory_and_queries_only_the_misses() {
        let (_dir, mut db, db_path) = setup("db-perf-batch");

        {
            let mut q = QueryBuilder::new(db.get_db());
            q.insert_nodes(&[make_node("n1"), make_node("n2"), make_node("n3")])
                .expect("nodes should insert");

            q.get_node_by_id("n1")
                .expect("n1 lookup should not throw")
                .expect("n1 should exist");
            update_node_name(&db_path, "n1", "changed");

            let ids = vec!["n1".to_owned(), "n2".to_owned()];
            let out = q.get_nodes_by_ids(&ids).expect("nodes should load");

            assert_eq!(out.get("n1").expect("n1 should exist").name, "n1");
            assert_eq!(out.get("n2").expect("n2 should exist").name, "n2");
        }

        db.close().expect("database should close");
    }
}

mod insert_node_cache_invalidation {
    use super::*;

    #[test]
    fn does_not_serve_a_stale_cached_node_after_insert_or_replace() {
        let (_dir, mut db, _db_path) = setup("db-perf-cache");

        {
            let mut q = QueryBuilder::new(db.get_db());
            let original = make_node_named("n1", "oldName");
            q.insert_node(&original)
                .expect("original node should insert");

            let before_replace = q
                .get_node_by_id("n1")
                .expect("node lookup should not throw")
                .expect("node should exist before replace");
            assert_eq!(before_replace.name, "oldName");

            let mut replacement = original.clone();
            replacement.name = "newName".to_owned();
            replacement.updated_at = now_ms();
            q.insert_node(&replacement)
                .expect("replacement node should insert");

            let after_replace = q
                .get_node_by_id("n1")
                .expect("node lookup should not throw")
                .expect("node should exist after replace");
            assert_eq!(after_replace.name, "newName");
        }

        db.close().expect("database should close");
    }
}

mod insert_edges_endpoint_validation {
    use super::*;

    #[test]
    fn skips_edges_with_missing_endpoints_instead_of_failing_the_whole_batch() {
        let (_dir, mut db, _db_path) = setup("db-perf-edges");

        {
            let mut q = QueryBuilder::new(db.get_db());
            q.insert_nodes(&[make_node("source"), make_node("target"), make_node("other")])
                .expect("nodes should insert");

            q.insert_edges(&[
                edge("source", "target", EdgeKind::Calls),
                edge("source", "missing-target", EdgeKind::Calls),
                edge("missing-source", "other", EdgeKind::References),
            ])
            .expect("dangling edges should be skipped without failing");

            let edges = q
                .get_outgoing_edges("source", None, None)
                .expect("outgoing edges should load");
            assert_eq!(edges.len(), 1);
            assert_eq!(edges[0].source, "source");
            assert_eq!(edges[0].target, "target");
            assert_eq!(edges[0].kind, EdgeKind::Calls);
        }

        db.close().expect("database should close");
    }

    #[test]
    fn does_not_trust_stale_cached_nodes_when_validating_edge_endpoints() {
        let (_dir, mut db, db_path) = setup("db-perf-edges");

        {
            let mut q = QueryBuilder::new(db.get_db());
            q.insert_nodes(&[make_node("source"), make_node("target")])
                .expect("nodes should insert");
            assert_eq!(
                q.get_node_by_id("target")
                    .expect("target lookup should not throw")
                    .expect("target should exist")
                    .id,
                "target"
            );

            delete_node_row(&db_path, "target");

            q.insert_edges(&[edge("source", "target", EdgeKind::Calls)])
                .expect("dangling edge should be skipped without failing");
            let edges = q
                .get_outgoing_edges("source", None, None)
                .expect("outgoing edges should load");
            assert!(edges.is_empty());
        }

        db.close().expect("database should close");
    }
}

mod run_maintenance {
    use super::*;

    #[test]
    fn runs_without_throwing_on_a_fresh_database() {
        let (_dir, mut db, _db_path) = setup("db-perf-maint");

        db.run_maintenance();

        db.close().expect("database should close");
    }

    #[test]
    fn runs_without_throwing_after_writes() {
        let (_dir, mut db, _db_path) = setup("db-perf-maint");

        {
            let mut q = QueryBuilder::new(db.get_db());
            q.insert_nodes(&[make_node("n1"), make_node("n2")])
                .expect("nodes should insert");
        }
        db.run_maintenance();

        db.close().expect("database should close");
    }

    #[test]
    fn swallows_failures_rather_than_propagating_best_effort() {
        let (_dir, mut db, _db_path) = setup("db-perf-maint");

        db.close()
            .expect("database should close before maintenance");
        db.run_maintenance();
    }
}
