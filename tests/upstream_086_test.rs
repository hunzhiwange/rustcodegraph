//! Terraform incremental graphs must match clean native-parser/SQLite rebuilds.
use rustcodegraph::{CodeGraph, IndexOptions};
use std::{collections::BTreeSet, path::Path};

struct Project(std::path::PathBuf);
impl Project {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "rcg-086-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).unwrap();
        let project = Self(path);
        project.write("main.tf", "module \"child\" { source = \"./child\" }\noutput \"value\" { value = module.child.id }");
        std::fs::create_dir_all(project.0.join("child")).unwrap();
        project.write(
            "child/main.tf",
            "locals { old = 1 }\noutput \"id\" { value = local.old }",
        );
        project.write("use.tf", "output \"other\" { value = module.child.id }");
        project.write("control.ts", "export function control() { return 86; }");
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
fn case_086_modified_declaration_matches_rebuild() {
    let project = Project::new();
    let mut graph = CodeGraph::init_sync(&project.0).unwrap();
    index(&mut graph);
    project.write(
        "child/main.tf",
        "locals { renamedDeclaration = 22 }\noutput \"id\" { value = local.renamedDeclaration }",
    );
    let sync = graph.sync(IndexOptions::default());
    assert_eq!(sync.files_modified, 1);
    assert!(
        !graph
            .get_nodes_in_file("child/main.tf")
            .iter()
            .any(|n| n.name == "old")
    );
    assert!(
        graph
            .get_nodes_in_file("child/main.tf")
            .iter()
            .any(|n| n.name == "renamedDeclaration")
    );
    matches_rebuild(&project, &mut graph);
    graph.close();
}
#[test]
fn case_086_deleted_import_target_matches_rebuild() {
    let project = Project::new();
    let mut graph = CodeGraph::init_sync(&project.0).unwrap();
    index(&mut graph);
    project.write(
        "child/main.tf",
        "locals { renamedDeclaration = 22 }\noutput \"id\" { value = local.renamedDeclaration }",
    );
    assert_eq!(graph.sync(IndexOptions::default()).files_modified, 1);
    std::fs::remove_file(project.0.join("child/main.tf")).unwrap();
    assert_eq!(graph.sync(IndexOptions::default()).files_removed, 1);
    assert!(graph.get_nodes_in_file("child/main.tf").is_empty());
    matches_rebuild(&project, &mut graph);
    graph.close();
}
#[test]
fn case_086_deleted_importer_and_noop_sync_match_rebuild() {
    let project = Project::new();
    let mut graph = CodeGraph::init_sync(&project.0).unwrap();
    index(&mut graph);
    std::fs::remove_file(project.0.join("main.tf")).unwrap();
    assert_eq!(graph.sync(IndexOptions::default()).files_removed, 1);
    assert!(graph.get_nodes_in_file("main.tf").is_empty());
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

#[test]
fn case_086_added_ambiguity_and_removed_candidate_match_rebuild() {
    let project = Project::new();
    project.write("variables.tf", "variable \"region\" {}");
    project.write("consumer.tf", "output \"region\" { value = var.region }");
    let mut graph = CodeGraph::init_sync(&project.0).unwrap();
    index(&mut graph);
    project.write("duplicate.tf", "variable \"region\" {}");
    assert_eq!(graph.sync(IndexOptions::default()).files_added, 1);
    matches_rebuild(&project, &mut graph);
    std::fs::remove_file(project.0.join("duplicate.tf")).unwrap();
    assert_eq!(graph.sync(IndexOptions::default()).files_removed, 1);
    matches_rebuild(&project, &mut graph);
    graph.close();
}

#[test]
fn case_086_provider_alias_edit_matches_rebuild() {
    let project = Project::new();
    project.write("providers.tf", "provider \"aws\" { alias = \"east\" }");
    project.write(
        "child/resource.tf",
        "resource \"aws_bucket\" \"x\" { provider = aws.east }",
    );
    let mut graph = CodeGraph::init_sync(&project.0).unwrap();
    index(&mut graph);
    project.write("providers.tf", "provider \"aws\" { alias = \"western\" }");
    assert_eq!(graph.sync(IndexOptions::default()).files_modified, 1);
    matches_rebuild(&project, &mut graph);
    graph.close();
}

#[test]
fn case_086_remote_component_change_clears_synthesized_edges() {
    let project = Project::new();
    std::fs::create_dir_all(project.0.join("components/vpc")).unwrap();
    project.write("components/vpc/out.tf", "output \"id\" { value = \"id\" }");
    project.write("remote.tf", "module \"state\" {\nsource = \"cloudposse/stack-config/yaml//modules/remote-state\"\ncomponent = var.component\n}\noutput \"consume\" { value = module.state.outputs.id }");
    project.write(
        "variables.tf",
        "variable \"component\" { default = \"vpc\" }",
    );
    let mut graph = CodeGraph::init_sync(&project.0).unwrap();
    index(&mut graph);
    assert!(
        snapshot(&project.0)
            .1
            .iter()
            .any(|e| e.contains("heuristic"))
    );
    project.write(
        "variables.tf",
        "variable \"component\" { default = \"absent\" }",
    );
    assert_eq!(graph.sync(IndexOptions::default()).files_modified, 1);
    assert!(
        !snapshot(&project.0)
            .1
            .iter()
            .any(|e| e.contains("heuristic"))
    );
    matches_rebuild(&project, &mut graph);
    graph.close();
}
