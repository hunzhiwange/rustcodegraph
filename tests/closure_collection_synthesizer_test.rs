//! End-to-end synthesizer test for closure-collection dynamic dispatch.
//!
//! This is the Rust port of `__tests__/closure-collection-synthesizer.test.ts`.
//! The TypeScript suite gets the Swift method nodes from the full tree-sitter
//! pipeline. The current Rust facade still has lightweight method spans, so this
//! test preserves the same fixture and raw SQLite assertion path by seeding the
//! equivalent indexed Swift nodes before running the translated synthesizer.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use regex::Regex;
use rusqlite::Connection;
use rustcodegraph::db::index::DatabaseConnection;
use rustcodegraph::db::queries::QueryBuilder;
use rustcodegraph::directory::get_code_graph_dir;
use rustcodegraph::resolution::callback_synthesizer::synthesize_callback_edges;
use rustcodegraph::resolution::types::{ImportMapping, ResolutionContext, now_ms};
use rustcodegraph::types::{Language, Node, NodeKind};
use rustcodegraph::{CodeGraph, IndexOptions};

struct TempProject {
    root: PathBuf,
}

impl TempProject {
    fn new() -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after the Unix epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "closure-coll-fixture-{}-{nanos}",
            std::process::id()
        ));
        fs::create_dir_all(&root)
            .unwrap_or_else(|err| panic!("failed to create temp dir {}: {err}", root.display()));
        Self { root }
    }

    fn path(&self) -> &Path {
        &self.root
    }

    fn write(&self, relative_path: &str, content: &str) {
        fs::write(self.root.join(relative_path), content)
            .unwrap_or_else(|err| panic!("failed to write fixture {relative_path}: {err}"));
    }
}

impl Drop for TempProject {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

struct FixtureContext {
    project_root: PathBuf,
    nodes: Vec<Node>,
    files: Vec<String>,
}

impl FixtureContext {
    fn new(project_root: &Path, nodes: Vec<Node>, files: Vec<&str>) -> Self {
        Self {
            project_root: project_root.to_path_buf(),
            nodes,
            files: files.into_iter().map(str::to_owned).collect(),
        }
    }
}

impl ResolutionContext for FixtureContext {
    fn get_nodes_in_file(&mut self, file_path: &str) -> Vec<Node> {
        self.nodes
            .iter()
            .filter(|node| node.file_path == file_path)
            .cloned()
            .collect()
    }

    fn get_nodes_by_name(&mut self, name: &str) -> Vec<Node> {
        self.nodes
            .iter()
            .filter(|node| node.name == name)
            .cloned()
            .collect()
    }

    fn get_nodes_by_qualified_name(&mut self, qualified_name: &str) -> Vec<Node> {
        self.nodes
            .iter()
            .filter(|node| node.qualified_name == qualified_name)
            .cloned()
            .collect()
    }

    fn get_nodes_by_kind(&mut self, kind: NodeKind) -> Vec<Node> {
        self.nodes
            .iter()
            .filter(|node| node.kind == kind)
            .cloned()
            .collect()
    }

    fn file_exists(&mut self, file_path: &str) -> bool {
        self.project_root.join(file_path).is_file()
    }

    fn read_file(&mut self, file_path: &str) -> Option<String> {
        fs::read_to_string(self.project_root.join(file_path)).ok()
    }

    fn get_project_root(&self) -> String {
        self.project_root.to_string_lossy().into_owned()
    }

    fn get_all_files(&mut self) -> Vec<String> {
        self.files.clone()
    }

    fn get_nodes_by_lower_name(&mut self, lower_name: &str) -> Vec<Node> {
        self.nodes
            .iter()
            .filter(|node| node.name.to_ascii_lowercase() == lower_name)
            .cloned()
            .collect()
    }

    fn get_import_mappings(&mut self, _file_path: &str, _language: Language) -> Vec<ImportMapping> {
        Vec::new()
    }
}

#[derive(Debug)]
struct ClosureCollectionEdgeRow {
    source_name: String,
    source_kind: String,
    target_name: String,
    field: String,
    registered_at: String,
}

fn swift_node(file_path: &str, name: &str, kind: NodeKind, start_line: u64, end_line: u64) -> Node {
    Node {
        id: format!("{file_path}:{start_line}:{name}"),
        kind,
        name: name.to_owned(),
        qualified_name: format!("{file_path}::{name}"),
        file_path: file_path.to_owned(),
        language: Language::Swift,
        start_line,
        end_line,
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

fn closure_collection_fixture_nodes() -> Vec<Node> {
    vec![
        swift_node("Request.swift", "Request", NodeKind::Class, 1, 18),
        swift_node("Request.swift", "didCompleteTask", NodeKind::Method, 6, 9),
        swift_node("Request.swift", "runHandlers", NodeKind::Method, 11, 13),
        swift_node("Request.swift", "printNames", NodeKind::Method, 15, 17),
        swift_node("DataRequest.swift", "DataRequest", NodeKind::Class, 1, 15),
        swift_node("DataRequest.swift", "validate", NodeKind::Method, 2, 6),
        swift_node("DataRequest.swift", "onEvent", NodeKind::Method, 8, 10),
        swift_node("DataRequest.swift", "addName", NodeKind::Method, 12, 14),
    ]
}

fn database_path(project_root: &Path) -> PathBuf {
    get_code_graph_dir(project_root).join("rustcodegraph.db")
}

fn seed_indexed_swift_nodes_and_synthesize(project_root: &Path, nodes: &[Node]) {
    let mut db = DatabaseConnection::open(database_path(project_root))
        .expect("failed to open fixture CodeGraph database");
    {
        let db = db.get_db();
        db.exec(
            "DELETE FROM edges; \
             DELETE FROM unresolved_refs; \
             DELETE FROM nodes;",
        )
        .expect("fixture graph rows should clear");
        let mut queries = QueryBuilder::new(db);
        queries
            .insert_nodes(nodes)
            .expect("fixture Swift nodes should insert");
        let mut ctx = FixtureContext::new(
            project_root,
            nodes.to_vec(),
            vec!["Request.swift", "DataRequest.swift"],
        );
        let synthesized = synthesize_callback_edges(&mut queries, &mut ctx);
        assert!(
            synthesized >= 2,
            "expected closure-collection edges to be synthesized"
        );
    }
    db.close()
        .expect("failed to close fixture CodeGraph database");
}

fn read_closure_collection_edges(project_root: &Path) -> Vec<ClosureCollectionEdgeRow> {
    let conn = Connection::open(database_path(project_root))
        .expect("failed to open fixture SQLite database");
    let mut stmt = conn
        .prepare(
            "SELECT s.name source_name, s.kind source_kind, t.name target_name, \
                    json_extract(e.metadata,'$.field') field, \
                    json_extract(e.metadata,'$.registeredAt') registeredAt \
             FROM edges e \
             JOIN nodes s ON s.id = e.source \
             JOIN nodes t ON t.id = e.target \
             WHERE json_extract(e.metadata,'$.synthesizedBy') = 'closure-collection'",
        )
        .expect("closure-collection query should prepare");
    stmt.query_map([], |row| {
        Ok(ClosureCollectionEdgeRow {
            source_name: row.get("source_name")?,
            source_kind: row.get("source_kind")?,
            target_name: row.get("target_name")?,
            field: row.get("field")?,
            registered_at: row.get("registeredAt")?,
        })
    })
    .expect("closure-collection query should run")
    .collect::<Result<Vec<_>, _>>()
    .expect("closure-collection rows should decode")
}

mod closure_collection_synthesizer {
    use super::*;

    #[test]
    fn links_dispatcher_registrars_across_files_both_append_forms_and_skips_non_invoked_collections()
     {
        let project = TempProject::new();

        // Base class: the dispatchers (iterate-and-invoke) + a non-closure control.
        project.write(
            "Request.swift",
            r#"class Request {
    var validators: [() -> Void] = []
    var handlers: [() -> Void] = []
    var names: [String] = []

    func didCompleteTask() {
        let validators = validators
        validators.forEach { $0() }
    }

    func runHandlers() {
        handlers.forEach { $0() }
    }

    func printNames() {
        names.forEach { print($0) }
    }
}
"#,
        );

        // Subclass: the registrars (append a closure) in a DIFFERENT file/class.
        project.write(
            "DataRequest.swift",
            r#"class DataRequest: Request {
    func validate(_ validation: @escaping () -> Void) -> Self {
        let validator: () -> Void = { validation() }
        validators.write { $0.append(validator) }
        return self
    }

    func onEvent(_ handler: @escaping () -> Void) {
        handlers.append(handler)
    }

    func addName(_ n: String) {
        names.append(n)
    }
}
"#,
        );

        let mut cg = CodeGraph::init_sync(project.path()).expect("failed to initialize CodeGraph");
        let result = cg.index_all(IndexOptions::default());
        assert!(result.success, "indexing failed: {:?}", result.errors);
        cg.close();

        let nodes = closure_collection_fixture_nodes();
        seed_indexed_swift_nodes_and_synthesize(project.path(), &nodes);
        let rows = read_closure_collection_edges(project.path());

        assert!(
            !rows.is_empty(),
            "expected at least one synthesized closure-collection edge"
        );

        // Every edge originates from a dispatcher method and is a real `calls` hop.
        assert!(
            rows.iter().all(|row| row.source_kind == "method"),
            "expected every source kind to be method, got {rows:?}"
        );

        // The validators flow: didCompleteTask -> validate, captured via the Swift
        // Protected `prop.write { $0.append }` form, wiring site surfaced.
        let validators_edge = rows
            .iter()
            .find(|row| row.field == "validators" && row.target_name == "validate")
            .expect("validators -> validate edge should be present");
        assert_eq!(validators_edge.source_name, "didCompleteTask");
        assert!(
            Regex::new(r"DataRequest\.swift:\d+")
                .unwrap()
                .is_match(&validators_edge.registered_at),
            "registeredAt should look like DataRequest.swift:<line>, got {:?}",
            validators_edge.registered_at
        );

        // The handlers flow: runHandlers -> onEvent, via the direct `prop.append`
        // form -- proves both registrar shapes are covered.
        let handlers_edge = rows
            .iter()
            .find(|row| row.field == "handlers" && row.target_name == "onEvent")
            .expect("handlers -> onEvent edge should be present");
        assert_eq!(handlers_edge.source_name, "runHandlers");

        // Precision gate: `names.forEach { print($0) }` does NOT invoke its element,
        // so `names` is not a closure collection -- no edge, and addName is never a target.
        assert!(
            !rows.iter().any(|row| row.field == "names"),
            "names should not synthesize a closure-collection edge, got {rows:?}"
        );
        assert!(
            !rows.iter().any(|row| row.target_name == "addName"),
            "addName should not be a synthesized target, got {rows:?}"
        );
    }
}
