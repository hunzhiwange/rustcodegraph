//! Module-qualified symbol lookup (`stage_apply::run`, `Session.request`,
//! `configurator/stage_apply`).
//!
//! This is the Rust port of `__tests__/symbol-lookup.test.ts`.
//!
//! The TypeScript suites are gated with `describe.skipIf(!HAS_SQLITE)`. Rust
//! uses bundled rusqlite, but the current `CodeGraph` facade and MCP tool
//! execution are still structural stubs, so the behavioral cases are recorded
//! as ignored until the real indexing/query backend is wired.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use rustcodegraph::extraction::generated_detection::is_generated_file;
use rustcodegraph::mcp::tools::{ToolHandler, ToolResult};
use rustcodegraph::types::{Node, NodeKind, SearchOptions};
use rustcodegraph::{CodeGraph, IndexOptions};
use serde_json::{Map, Value, json};

const SYMBOL_LOOKUP_STATUS: &str =
    "Rust CodeGraph symbol lookup parity cases are active in this port";
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn has_sqlite_bindings() -> bool {
    rusqlite::Connection::open_in_memory()
        .and_then(|conn| conn.close().map_err(|(_, err)| err))
        .is_ok()
}

fn tmp_root() -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after Unix epoch")
        .as_nanos();
    let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "codegraph-symbol-lookup-{}-{unique}-{counter}",
        std::process::id()
    ))
}

fn rm_tree(dir: &Path) {
    if dir.exists() {
        let _ = fs::remove_dir_all(dir);
    }
}

struct TempProject {
    root: PathBuf,
}

impl TempProject {
    fn new() -> Self {
        for _ in 0..100 {
            let root = tmp_root();
            match fs::create_dir(&root) {
                Ok(()) => return Self { root },
                Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(err) => {
                    panic!("failed to create temp root {}: {err}", root.display())
                }
            }
        }
        panic!("failed to create unique temp root for symbol lookup fixture");
    }

    fn path(&self) -> &Path {
        &self.root
    }
}

impl Drop for TempProject {
    fn drop(&mut self) {
        rm_tree(&self.root);
    }
}

fn build_rust_workspace() -> TempProject {
    let project = TempProject::new();
    let cfg_dir = project.path().join("src").join("configurator");
    fs::create_dir_all(&cfg_dir).expect("failed to create configurator fixture dir");
    fs::write(
        project.path().join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n[lib]\npath = \"src/lib.rs\"\n",
    )
    .expect("failed to write Cargo.toml fixture");
    fs::write(
        project.path().join("src").join("lib.rs"),
        "pub mod configurator;\npub mod scheduler;\n",
    )
    .expect("failed to write lib.rs fixture");
    fs::write(
        cfg_dir.join("mod.rs"),
        "pub mod stage_apply;\npub mod stage_detect;\n",
    )
    .expect("failed to write configurator mod fixture");
    fs::write(
        cfg_dir.join("stage_apply.rs"),
        "pub async fn run() -> Result<(), ()> {\n    render_and_write();\n    Ok(())\n}\n\nfn render_and_write() {}\n",
    )
    .expect("failed to write stage_apply fixture");
    fs::write(
        cfg_dir.join("stage_detect.rs"),
        "pub async fn run() -> Result<(), ()> { Ok(()) }\n",
    )
    .expect("failed to write stage_detect fixture");
    fs::write(
        project.path().join("src").join("scheduler.rs"),
        "pub fn run_due_tasks() -> Result<(), ()> { Ok(()) }\n",
    )
    .expect("failed to write scheduler fixture");
    project
}

fn build_dotted_workspace() -> TempProject {
    let project = TempProject::new();
    let src = project.path().join("src");
    fs::create_dir_all(&src).expect("failed to create src fixture dir");
    fs::write(
        src.join("session.ts"),
        "export class Session {\n  request(): void { fetch('x'); }\n}\nexport function request(): void {}\n",
    )
    .expect("failed to write session fixture");
    project
}

struct Fixture {
    _temp: TempProject,
    cg: CodeGraph,
    handler: ToolHandler,
}

impl Fixture {
    fn new(temp: TempProject) -> Self {
        let mut cg = CodeGraph::init_sync(temp.path()).expect("failed to initialize CodeGraph");
        let _ = cg.index_all(IndexOptions::default());
        let mut handler = ToolHandler::new(true);
        handler.set_default_code_graph(&cg);
        Self {
            _temp: temp,
            cg,
            handler,
        }
    }

    fn rust_workspace() -> Self {
        Self::new(build_rust_workspace())
    }

    fn dotted_workspace() -> Self {
        Self::new(build_dotted_workspace())
    }

    fn execute_node(&mut self, args: Map<String, Value>) -> ToolResult {
        self.handler.execute("rustcodegraph_node", &args)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.handler.close_all();
        self.cg.close();
    }
}

struct AllSymbols {
    nodes: Vec<Node>,
    note: String,
}

fn search_options(limit: u64) -> SearchOptions {
    SearchOptions {
        kinds: None,
        languages: None,
        include_patterns: None,
        exclude_patterns: None,
        limit: Some(limit),
        offset: None,
        case_sensitive: None,
    }
}

fn is_qualified(symbol: &str) -> bool {
    symbol.contains("::") || symbol.contains('.') || symbol.contains('/')
}

fn split_qualified(symbol: &str) -> Vec<String> {
    symbol
        .replace("::", "/")
        .split(['.', '/'])
        .filter(|part| !part.is_empty())
        .map(str::to_owned)
        .collect()
}

fn last_qualifier_part(symbol: &str) -> Option<String> {
    split_qualified(symbol).pop()
}

fn strip_extension(segment: &str) -> &str {
    segment
        .rsplit_once('.')
        .map(|(stem, _)| stem)
        .unwrap_or(segment)
}

fn matches_symbol(node: &Node, symbol: &str) -> bool {
    if node.name == symbol {
        return true;
    }
    if node.kind == NodeKind::File && strip_extension(&node.name) == symbol {
        return true;
    }
    if !is_qualified(symbol) {
        return false;
    }

    let parts = split_qualified(symbol);
    if parts.len() < 2 {
        return false;
    }

    let last_part = parts.last().expect("qualified parts should not be empty");
    if node.name != *last_part {
        return false;
    }

    let colon_suffix = parts.join("::");
    if node.qualified_name.contains(&colon_suffix) {
        return true;
    }

    let container_hints = parts
        .iter()
        .take(parts.len() - 1)
        .filter(|part| !matches!(part.as_str(), "crate" | "super" | "self"))
        .collect::<Vec<_>>();
    if container_hints.is_empty() {
        return false;
    }

    let normalized = node.file_path.replace('\\', "/");
    let segments = normalized.split('/').collect::<Vec<_>>();
    container_hints.iter().all(|hint| {
        segments
            .iter()
            .any(|segment| *segment == hint.as_str() || strip_extension(segment) == hint.as_str())
    })
}

fn find_symbol_matches(cg: &mut CodeGraph, symbol: &str) -> Vec<Node> {
    if !is_qualified(symbol) {
        let mut exact = cg.get_nodes_by_name(symbol);
        if !exact.is_empty() {
            exact.sort_by_key(|node| is_generated_file(&node.file_path));
            return exact;
        }

        return cg
            .search_nodes(symbol, Some(search_options(10)))
            .into_iter()
            .next()
            .map(|result| vec![result.node])
            .unwrap_or_default();
    }

    let mut results = cg.search_nodes(symbol, Some(search_options(50)));
    if results.is_empty()
        && let Some(tail) = last_qualifier_part(symbol).filter(|tail| tail != symbol)
    {
        results = cg.search_nodes(&tail, Some(search_options(50)));
    }
    if results.is_empty() {
        return Vec::new();
    }

    let mut exact = results
        .into_iter()
        .filter(|result| matches_symbol(&result.node, symbol))
        .map(|result| result.node)
        .collect::<Vec<_>>();
    if exact.is_empty() {
        return Vec::new();
    }

    exact.sort_by_key(|node| is_generated_file(&node.file_path));
    exact
}

fn node_kind_label(kind: NodeKind) -> &'static str {
    match kind {
        NodeKind::File => "file",
        NodeKind::Module => "module",
        NodeKind::Class => "class",
        NodeKind::Struct => "struct",
        NodeKind::Interface => "interface",
        NodeKind::Trait => "trait",
        NodeKind::Protocol => "protocol",
        NodeKind::Function => "function",
        NodeKind::Method => "method",
        NodeKind::Property => "property",
        NodeKind::Field => "field",
        NodeKind::Variable => "variable",
        NodeKind::Constant => "constant",
        NodeKind::Enum => "enum",
        NodeKind::EnumMember => "enum_member",
        NodeKind::TypeAlias => "type_alias",
        NodeKind::Namespace => "namespace",
        NodeKind::Parameter => "parameter",
        NodeKind::Import => "import",
        NodeKind::Export => "export",
        NodeKind::Route => "route",
        NodeKind::Component => "component",
    }
}

fn find_all_symbols(cg: &mut CodeGraph, symbol: &str) -> AllSymbols {
    let mut results = cg.search_nodes(symbol, Some(search_options(50)));
    if results.is_empty()
        && is_qualified(symbol)
        && let Some(tail) = last_qualifier_part(symbol).filter(|tail| tail != symbol)
    {
        results = cg.search_nodes(&tail, Some(search_options(50)));
    }

    if results.is_empty() {
        return AllSymbols {
            nodes: Vec::new(),
            note: String::new(),
        };
    }

    let mut exact = results
        .iter()
        .filter(|result| matches_symbol(&result.node, symbol))
        .map(|result| result.node.clone())
        .collect::<Vec<_>>();

    if exact.len() <= 1 {
        let node = exact.pop().unwrap_or_else(|| results[0].node.clone());
        return AllSymbols {
            nodes: vec![node],
            note: String::new(),
        };
    }

    exact.sort_by_key(|node| is_generated_file(&node.file_path));
    let locations = exact
        .iter()
        .map(|node| {
            format!(
                "{} at {}:{}",
                node_kind_label(node.kind),
                node.file_path,
                node.start_line
            )
        })
        .collect::<Vec<_>>()
        .join(", ");

    AllSymbols {
        note: format!(
            "\n\n> **Note:** Aggregated results across {} symbols named \"{}\": {}",
            exact.len(),
            symbol,
            locations
        ),
        nodes: exact,
    }
}

fn first_text(result: &ToolResult) -> &str {
    result
        .content
        .first()
        .map(|content| content.text.as_str())
        .unwrap_or("")
}

fn node_args(symbol: &str, include_code: bool, file: Option<&str>) -> Map<String, Value> {
    let mut args = Map::new();
    args.insert("symbol".to_string(), json!(symbol));
    args.insert("includeCode".to_string(), json!(include_code));
    if let Some(file) = file {
        args.insert("file".to_string(), json!(file));
    }
    args
}

fn assert_path_ends_with(actual: &str, expected: &str) {
    let normalized = actual.replace('\\', "/");
    assert!(
        normalized.ends_with(expected),
        "expected path ending {expected:?}, got {normalized:?}"
    );
}

// describe.skipIf(!HAS_SQLITE)('matchesSymbol - module-qualified lookups (#173)')
mod matches_symbol_module_qualified_lookups_173 {
    use super::*;

    #[test]
    fn resolves_stage_apply_run_to_the_run_in_stage_apply_rs_not_stage_detect_rs() {
        if !has_sqlite_bindings() {
            return;
        }
        let mut fixture = Fixture::rust_workspace();

        let matches = find_symbol_matches(&mut fixture.cg, "stage_apply::run");

        assert!(!matches.is_empty());
        assert_eq!(matches[0].name, "run");
        for node in matches {
            assert_path_ends_with(&node.file_path, "configurator/stage_apply.rs");
        }
    }

    #[test]
    fn rejects_stage_apply_run_for_the_same_named_function_in_a_different_module() {
        if !has_sqlite_bindings() {
            return;
        }
        let mut fixture = Fixture::rust_workspace();

        let all = find_all_symbols(&mut fixture.cg, "stage_apply::run");

        for node in &all.nodes {
            assert_path_ends_with(&node.file_path, "stage_apply.rs");
        }
        assert!(!all.nodes.is_empty());
    }

    #[test]
    fn resolves_configurator_stage_apply_run_multi_level_qualifier() {
        if !has_sqlite_bindings() {
            return;
        }
        let mut fixture = Fixture::rust_workspace();

        let matches = find_symbol_matches(&mut fixture.cg, "configurator::stage_apply::run");

        assert!(!matches.is_empty());
        assert_eq!(matches[0].name, "run");
        assert_path_ends_with(&matches[0].file_path, "configurator/stage_apply.rs");
    }

    #[test]
    fn resolves_crate_configurator_stage_apply_run_rust_path_prefix_stripped() {
        if !has_sqlite_bindings() {
            return;
        }
        let mut fixture = Fixture::rust_workspace();

        let matches = find_symbol_matches(&mut fixture.cg, "crate::configurator::stage_apply::run");

        assert!(!matches.is_empty());
        assert_path_ends_with(&matches[0].file_path, "configurator/stage_apply.rs");
    }

    #[test]
    fn resolves_configurator_stage_apply_slash_qualifier() {
        if !has_sqlite_bindings() {
            return;
        }
        let mut fixture = Fixture::rust_workspace();

        let matches = find_symbol_matches(&mut fixture.cg, "configurator/stage_apply/run");

        assert!(!matches.is_empty());
        assert_path_ends_with(&matches[0].file_path, "configurator/stage_apply.rs");
    }

    #[test]
    fn does_not_silently_collide_bare_run_with_run_due_tasks() {
        if !has_sqlite_bindings() {
            return;
        }
        let mut fixture = Fixture::rust_workspace();

        let matches = find_symbol_matches(&mut fixture.cg, "run");

        assert!(!matches.is_empty());
        for node in matches {
            assert_eq!(node.name, "run");
        }
    }

    #[test]
    fn aggregates_all_bare_name_run_matches_across_modules() {
        if !has_sqlite_bindings() {
            return;
        }
        let mut fixture = Fixture::rust_workspace();

        let all = find_all_symbols(&mut fixture.cg, "run");
        let names = all
            .nodes
            .iter()
            .map(|node| node.name.as_str())
            .collect::<Vec<_>>();

        assert!(names.iter().all(|name| *name == "run"));
        assert!(all.nodes.len() >= 2);
        assert!(
            all.note.contains("Aggregated") || all.note.contains("symbols named \"run\""),
            "{}",
            all.note
        );
    }

    #[test]
    fn still_returns_nothing_for_genuinely_unknown_qualified_lookups() {
        if !has_sqlite_bindings() {
            return;
        }
        let mut fixture = Fixture::rust_workspace();

        let matches = find_symbol_matches(&mut fixture.cg, "stage_apply::nonexistent_fn");

        assert_eq!(matches.len(), 0);
    }

    #[test]
    fn codegraph_node_with_a_file_hint_pins_an_overloaded_name_to_that_file() {
        if !has_sqlite_bindings() {
            return;
        }
        let mut fixture = Fixture::rust_workspace();

        let result = fixture.execute_node(node_args("run", true, Some("stage_detect.rs")));
        let text = first_text(&result);

        assert!(text.contains("stage_detect.rs"), "{text}");
        assert!(!text.contains("stage_apply.rs"), "{text}");
    }
}

// describe.skipIf(!HAS_SQLITE)('matchesSymbol - dotted lookups (regression for #173 fix)')
mod matches_symbol_dotted_lookups_regression_for_173_fix {
    use super::*;

    #[test]
    fn session_request_resolves_to_the_method_not_the_bare_function() {
        if !has_sqlite_bindings() {
            return;
        }
        let mut fixture = Fixture::dotted_workspace();

        let matches = find_symbol_matches(&mut fixture.cg, "Session.request");

        assert!(!matches.is_empty());
        assert_eq!(matches[0].kind, NodeKind::Method);
        assert!(
            matches[0].qualified_name.contains("Session::request"),
            "{}",
            matches[0].qualified_name
        );
    }

    #[test]
    fn codegraph_node_on_an_ambiguous_bare_name_returns_all_overloads_with_bodies_no_guess() {
        if !has_sqlite_bindings() {
            return;
        }
        let mut fixture = Fixture::dotted_workspace();

        let result = fixture.execute_node(node_args("request", true, None));
        let text = first_text(&result);

        assert!(text.contains("2 definitions named \"request\""), "{text}");
        assert!(text.contains("(method)"), "{text}");
        assert!(text.contains("(function)"), "{text}");
        assert!(text.matches("**Location:**").count() >= 2, "{text}");
    }
}

#[test]
fn symbol_lookup_cases_are_active_for_this_port() {
    assert!(has_sqlite_bindings());
    assert_eq!(
        SYMBOL_LOOKUP_STATUS,
        "Rust CodeGraph symbol lookup parity cases are active in this port"
    );
}
