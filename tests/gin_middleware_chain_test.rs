//! End-to-end synthesizer test for the gin middleware chain.
//!
//! This is the Rust port of `__tests__/gin-middleware-chain.test.ts`.
//! The TypeScript suite gets the Go nodes from the full tree-sitter pipeline.
//! The current Rust facade still has only lightweight indexing, so this test
//! preserves the same fixture and raw SQLite assertion path by seeding the
//! equivalent indexed Go nodes before running the translated synthesizer.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::Connection;
use rustcodegraph::db::index::DatabaseConnection;
use rustcodegraph::db::queries::QueryBuilder;
use rustcodegraph::directory::get_code_graph_dir;
use rustcodegraph::extraction::tree_sitter_helpers::generate_node_id;
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
        let root =
            std::env::temp_dir().join(format!("gin-chain-fixture-{}-{nanos}", std::process::id()));
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
struct GinEdgeRow {
    source_name: String,
    source_kind: String,
    target_name: String,
    via: String,
    registered_at: String,
}

fn go_node(file_path: &str, name: &str, kind: NodeKind, start_line: u64, end_line: u64) -> Node {
    Node {
        id: generate_node_id(file_path, kind, name, start_line as usize),
        kind,
        name: name.to_owned(),
        qualified_name: format!("{file_path}::{name}"),
        file_path: file_path.to_owned(),
        language: Language::Go,
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

fn gin_fixture_nodes() -> Vec<Node> {
    vec![
        go_node("gin.go", "Context", NodeKind::Struct, 6, 9),
        go_node("gin.go", "Context.Next", NodeKind::Method, 11, 17),
        go_node("gin.go", "Engine", NodeKind::Struct, 19, 21),
        go_node("gin.go", "Engine.Use", NodeKind::Method, 23, 25),
        go_node("gin.go", "Engine.GET", NodeKind::Method, 27, 27),
        go_node("app.go", "Logger", NodeKind::Function, 3, 3),
        go_node("app.go", "Recovery", NodeKind::Function, 4, 4),
        go_node("app.go", "getUser", NodeKind::Function, 5, 5),
        go_node("app.go", "setup", NodeKind::Function, 7, 12),
    ]
}

fn database_path(project_root: &Path) -> PathBuf {
    get_code_graph_dir(project_root).join("rustcodegraph.db")
}

fn seed_indexed_go_nodes_and_synthesize(project_root: &Path, nodes: &[Node]) {
    let mut db = DatabaseConnection::open(database_path(project_root))
        .expect("failed to open fixture CodeGraph database");
    {
        let mut queries = QueryBuilder::new(db.get_db());
        queries
            .insert_nodes(nodes)
            .expect("fixture Go nodes should insert");
        let mut ctx = FixtureContext::new(project_root, nodes.to_vec(), vec!["gin.go", "app.go"]);
        let synthesized = synthesize_callback_edges(&mut queries, &mut ctx);
        assert!(
            synthesized >= 3,
            "expected gin middleware-chain edges to be synthesized"
        );
    }
    db.close()
        .expect("failed to close fixture CodeGraph database");
}

fn read_gin_edges(project_root: &Path) -> Vec<GinEdgeRow> {
    let conn = Connection::open(database_path(project_root))
        .expect("failed to open fixture SQLite database");
    let mut stmt = conn
        .prepare(
            "SELECT s.name source_name, s.kind source_kind, t.name target_name, \
                    json_extract(e.metadata,'$.via') via, \
                    json_extract(e.metadata,'$.registeredAt') registeredAt \
             FROM edges e \
             JOIN nodes s ON s.id = e.source \
             JOIN nodes t ON t.id = e.target \
             WHERE json_extract(e.metadata,'$.synthesizedBy') = 'gin-middleware-chain'",
        )
        .expect("gin middleware-chain query should prepare");
    stmt.query_map([], |row| {
        Ok(GinEdgeRow {
            source_name: row.get("source_name")?,
            source_kind: row.get("source_kind")?,
            target_name: row.get("target_name")?,
            via: row.get("via")?,
            registered_at: row.get("registeredAt")?,
        })
    })
    .expect("gin middleware-chain query should run")
    .collect::<Result<Vec<_>, _>>()
    .expect("gin middleware-chain rows should decode")
}

mod gin_middleware_chain_synthesizer {
    use super::*;

    #[test]
    fn links_context_next_to_handlers_registered_via_use_get_and_skips_inline_closures() {
        let project = TempProject::new();
        project.write("go.mod", "module ginapp\n\ngo 1.21\n");

        // gin-core shape: the dynamic-dispatch chain driver + registration surface.
        project.write(
            "gin.go",
            r#"package gin

type HandlerFunc func(*Context)
type HandlersChain []HandlerFunc

type Context struct {
	handlers HandlersChain
	index    int8
}

func (c *Context) Next() {
	c.index++
	for c.index < int8(len(c.handlers)) {
		c.handlers[c.index](c)
		c.index++
	}
}

type Engine struct {
	Handlers HandlersChain
}

func (e *Engine) Use(middleware ...HandlerFunc) {
	e.Handlers = append(e.Handlers, middleware...)
}

func (e *Engine) GET(path string, handlers ...HandlerFunc) {}
"#,
        );

        // registration site: named middleware + named route handler + an inline closure.
        project.write(
            "app.go",
            r#"package gin

func Logger(c *Context)   {}
func Recovery(c *Context) {}
func getUser(c *Context)  {}

func setup() {
	e := &Engine{}
	e.Use(Logger, Recovery)
	e.GET("/users", getUser)
	e.GET("/inline", func(c *Context) {})
}
"#,
        );

        let mut cg = CodeGraph::init_sync(project.path()).expect("failed to initialize CodeGraph");
        let result = cg.index_all(IndexOptions::default());
        assert!(result.success, "indexing failed: {:?}", result.errors);
        cg.close();

        let nodes = gin_fixture_nodes();
        seed_indexed_go_nodes_and_synthesize(project.path(), &nodes);
        let rows = read_gin_edges(project.path());

        // Every edge originates from the chain dispatcher Context.Next.
        assert!(!rows.is_empty(), "expected at least one synthesized edge");
        assert!(
            rows.iter()
                .all(|row| row.source_name == "Context.Next" && row.source_kind == "method"),
            "expected every edge to originate from Context.Next, got {rows:?}"
        );

        // Exactly the three NAMED handlers are linked -- the inline closure
        // registration is anonymous and must be skipped.
        let targets = rows
            .iter()
            .map(|row| row.target_name.as_str())
            .collect::<HashSet<_>>();
        assert_eq!(targets, HashSet::from(["Logger", "Recovery", "getUser"]));

        // The wiring site (`.Use`/`.GET` call) is surfaced for the agent.
        let logger = rows
            .iter()
            .find(|row| row.target_name == "Logger")
            .expect("Logger edge should be present");
        assert_eq!(logger.via, "Logger");
        assert!(
            logger
                .registered_at
                .strip_prefix("app.go:")
                .and_then(|line| line.parse::<u64>().ok())
                .is_some(),
            "registeredAt should look like app.go:<line>, got {:?}",
            logger.registered_at
        );
    }
}
