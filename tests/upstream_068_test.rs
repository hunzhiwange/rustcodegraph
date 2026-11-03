//! Nix incremental graphs must match clean native-parser/SQLite rebuilds.
use rustcodegraph::{CodeGraph, IndexOptions};
use std::{collections::BTreeSet, path::Path};

struct Project(std::path::PathBuf);
impl Project {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "rcg-068-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).unwrap();
        let project = Self(path);
        project.write("main.nix", "{ load = x: import ./module.nix; }");
        project.write("module.nix", "{ old = x: x; }");
        // A second importer boundary must survive when main.nix is re-extracted.
        project.write("entry.nix", "{ entry = x: import ./main.nix; }");
        project.write("control.ts", "export function control() { return 68; }");
        project
    }
    fn write(&self, file: &str, content: &str) {
        std::fs::write(self.0.join(file), content).unwrap();
    }
}
impl Drop for Project {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

// IDs and timestamps are storage details; compare stable symbol keys and edge endpoints.
fn snapshot(root: &Path) -> (BTreeSet<String>, BTreeSet<String>) {
    let db = rusqlite::Connection::open(root.join(".rustcodegraph/rustcodegraph.db")).unwrap();
    let key = |alias: &str| {
        format!(
            "{alias}.file_path || ':' || {alias}.kind || ':' || {alias}.qualified_name || ':' || {alias}.start_line"
        )
    };
    let collect = |sql: String| -> BTreeSet<String> {
        db.prepare(&sql)
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    };
    let dangling: i64 = db.query_row("SELECT count(*) FROM edges e LEFT JOIN nodes s ON s.id=e.source LEFT JOIN nodes t ON t.id=e.target WHERE s.id IS NULL OR t.id IS NULL", [], |r| r.get(0)).unwrap();
    assert_eq!(dangling, 0);
    (
        collect(format!("SELECT {} FROM nodes n", key("n"))),
        collect(format!(
            "SELECT {} || ' -> ' || e.kind || ' -> ' || {} || ':' || COALESCE(e.provenance, '') FROM edges e JOIN nodes s ON s.id=e.source JOIN nodes t ON t.id=e.target",
            key("s"),
            key("t")
        )),
    )
}
fn index(graph: &mut CodeGraph) {
    let result = graph.index_all(IndexOptions::default());
    assert!(result.success, "{:?}", result.errors);
    assert_eq!(result.files_errored, 0);
}
fn matches_rebuild(project: &Project, graph: &mut CodeGraph) {
    let incremental = snapshot(&project.0);
    assert!(
        incremental
            .0
            .iter()
            .any(|key| key.contains("control.ts:function:control"))
    );
    graph.close();
    std::fs::remove_dir_all(project.0.join(".rustcodegraph")).unwrap();
    *graph = CodeGraph::init_sync(&project.0).unwrap();
    index(graph);
    assert_eq!(incremental, snapshot(&project.0));
}
#[test]
fn case_068_modified_declaration_matches_rebuild() {
    let project = Project::new();
    let mut graph = CodeGraph::init_sync(&project.0).unwrap();
    index(&mut graph);
    project.write("module.nix", "{ renamedDeclaration = x: x; }");
    let sync = graph.sync(IndexOptions::default());
    assert_eq!(sync.files_modified, 1);
    assert!(
        !graph
            .get_nodes_in_file("module.nix")
            .iter()
            .any(|n| n.name == "old")
    );
    assert!(
        graph
            .get_nodes_in_file("module.nix")
            .iter()
            .any(|n| n.name == "renamedDeclaration")
    );
    matches_rebuild(&project, &mut graph);
    graph.close();
}
#[test]
fn case_068_deleted_import_target_matches_rebuild() {
    let project = Project::new();
    let mut graph = CodeGraph::init_sync(&project.0).unwrap();
    index(&mut graph);
    project.write("module.nix", "{ renamedDeclaration = x: x; }");
    assert_eq!(graph.sync(IndexOptions::default()).files_modified, 1);
    std::fs::remove_file(project.0.join("module.nix")).unwrap();
    assert_eq!(graph.sync(IndexOptions::default()).files_removed, 1);
    assert!(graph.get_nodes_in_file("module.nix").is_empty());
    matches_rebuild(&project, &mut graph);
    graph.close();
}
#[test]
fn case_068_deleted_importer_and_noop_sync_match_rebuild() {
    let project = Project::new();
    let mut graph = CodeGraph::init_sync(&project.0).unwrap();
    index(&mut graph);
    std::fs::remove_file(project.0.join("main.nix")).unwrap();
    assert_eq!(graph.sync(IndexOptions::default()).files_removed, 1);
    assert!(graph.get_nodes_in_file("main.nix").is_empty());
    let before = snapshot(&project.0);
    let noop = graph.sync(IndexOptions::default());
    assert_eq!(
        (noop.files_added, noop.files_modified, noop.files_removed),
        (0, 0, 0)
    );
    assert_eq!(before, snapshot(&project.0));
    matches_rebuild(&project, &mut graph);
    graph.close();
}
