//! `QueryBuilder::iterate_nodes_by_kind` -- the streaming scan that fixes the
//! #610 OOM.
//!
//! This is the Rust port of `__tests__/iterate-nodes-by-kind.test.ts`.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use rustcodegraph::db::index::DatabaseConnection;
use rustcodegraph::db::queries::QueryBuilder;
use rustcodegraph::types::{Node, NodeKind};
use rustcodegraph::{CodeGraph, IndexOptions};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

struct TempProject {
    root: PathBuf,
}

impl TempProject {
    fn new(prefix: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after the Unix epoch")
            .as_nanos();
        let suffix = TEMP_COUNTER.fetch_add(1, Ordering::SeqCst);
        let root =
            env::temp_dir().join(format!("{prefix}-{}-{nanos}-{suffix}", std::process::id()));
        fs::create_dir_all(root.join("src")).expect("failed to create fixture src dir");
        Self { root }
    }

    fn path(&self) -> &Path {
        &self.root
    }

    fn write_src(&self, name: &str, body: &str) {
        fs::write(self.root.join("src").join(name), body)
            .expect("failed to write fixture source file");
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
    temp: TempProject,
    cg: CodeGraph,
}

impl Fixture {
    fn new() -> Self {
        let temp = TempProject::new("cg-iter");
        temp.write_src(
            "a.ts",
            "export function foo() { return 1; }\n\
             export function bar() { return 2; }\n\
             export class C { m() { return 3; } n() { return 4; } }\n",
        );

        let mut cg = CodeGraph::init_sync(temp.path()).expect("failed to initialize CodeGraph");
        let result = cg.index_all(IndexOptions::default());
        assert!(result.success, "indexing failed: {:?}", result.errors);

        Self { temp, cg }
    }

    fn path(&self) -> &Path {
        self.temp.path()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.cg.close();
    }
}

fn database_path(project_root: &Path) -> PathBuf {
    project_root.join(".rustcodegraph").join("rustcodegraph.db")
}

fn with_queries<R>(project_root: &Path, f: impl FnOnce(&mut QueryBuilder<'_>) -> R) -> R {
    let mut db = DatabaseConnection::open(database_path(project_root))
        .expect("failed to open fixture CodeGraph database");
    let result = {
        let mut queries = QueryBuilder::new(db.get_db());
        f(&mut queries)
    };
    db.close()
        .expect("failed to close fixture CodeGraph database");
    result
}

fn sorted_ids(nodes: Vec<Node>) -> Vec<String> {
    let mut ids = nodes.into_iter().map(|node| node.id).collect::<Vec<_>>();
    ids.sort();
    ids
}

fn eager_ids(queries: &mut QueryBuilder<'_>, kind: NodeKind) -> Vec<String> {
    sorted_ids(
        queries
            .get_nodes_by_kind(kind)
            .unwrap_or_else(|err| panic!("failed to eagerly query {kind:?} nodes: {err}")),
    )
}

fn streamed_ids(queries: &mut QueryBuilder<'_>, kind: NodeKind) -> Vec<String> {
    let mut ids = queries
        .iterate_nodes_by_kind(kind)
        .unwrap_or_else(|err| panic!("failed to stream {kind:?} nodes: {err}"))
        .map(|result| {
            result
                .unwrap_or_else(|err| panic!("failed to read streamed {kind:?} node: {err}"))
                .id
        })
        .collect::<Vec<_>>();
    ids.sort();
    ids
}

mod iterate_nodes_by_kind_610_streaming {
    use super::*;

    #[test]
    fn yields_exactly_the_same_nodes_as_the_eager_get_nodes_by_kind() {
        let fixture = Fixture::new();
        with_queries(fixture.path(), |queries| {
            for kind in [NodeKind::Function, NodeKind::Method, NodeKind::Class] {
                let eager = eager_ids(queries, kind);
                let streamed = streamed_ids(queries, kind);
                assert_eq!(
                    streamed, eager,
                    "streamed {kind:?} ids should match eager ids"
                );
            }

            // Sanity: the fixture actually produced functions + methods to stream.
            assert!(
                !streamed_ids(queries, NodeKind::Function).is_empty(),
                "fixture should produce function nodes"
            );
            assert!(
                !streamed_ids(queries, NodeKind::Method).is_empty(),
                "fixture should produce method nodes"
            );
        });
    }

    #[test]
    fn keeps_the_cursor_valid_while_other_queries_run_mid_iteration() {
        let fixture = Fixture::new();
        with_queries(fixture.path(), |queries| {
            let mut seen = 0;
            let iterator = queries
                .iterate_nodes_by_kind(NodeKind::Function)
                .expect("failed to stream function nodes");

            for node in iterator {
                let node = node.expect("failed to read streamed function node");
                // A different prepared statement stepped on the same connection while the
                // iterator's cursor is open must not corrupt it.
                let again = queries
                    .get_node_by_id(&node.id)
                    .expect("failed to query node by id");
                assert_eq!(
                    again.as_ref().map(|node| node.id.as_str()),
                    Some(node.id.as_str())
                );
                seen += 1;
            }

            assert_eq!(
                seen,
                queries
                    .get_nodes_by_kind(NodeKind::Function)
                    .expect("failed to eagerly query function nodes")
                    .len()
            );
        });
    }
}
