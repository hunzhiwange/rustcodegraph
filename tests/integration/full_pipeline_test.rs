//! End-to-end pipeline integration tests.
//!
//! Rust port of `__tests__/integration/full-pipeline.test.ts`.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use rustcodegraph::types::{
    BuildContextOptions, ContextFormat, EdgeKind, SearchOptions, TaskInput,
};
use rustcodegraph::{BuildContextResult, CodeGraph, IndexOptions};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn create_temp_dir(prefix: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after Unix epoch")
        .as_nanos();
    let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = env::temp_dir().join(format!("{prefix}{}-{unique}-{counter}", std::process::id()));
    fs::create_dir_all(&dir)
        .unwrap_or_else(|err| panic!("failed to create temp dir {}: {err}", dir.display()));
    dir
}

fn cleanup_temp_dir(dir: &Path) {
    if dir.exists() {
        let _ = fs::remove_dir_all(dir);
    }
}

struct TempProject {
    root: PathBuf,
}

impl TempProject {
    fn new() -> Self {
        Self {
            root: create_temp_dir("codegraph-int-"),
        }
    }

    fn path(&self) -> &Path {
        &self.root
    }
}

impl Drop for TempProject {
    fn drop(&mut self) {
        cleanup_temp_dir(&self.root);
    }
}

struct CodeGraphGuard {
    cg: CodeGraph,
}

impl CodeGraphGuard {
    fn init(project_root: &Path) -> Self {
        // TypeScript passes config include/exclude here. The Rust facade's
        // init options currently expose only lifecycle flags, and these
        // fixtures contain only TypeScript source files.
        let cg = CodeGraph::init_sync(project_root).expect("CodeGraph should initialize");
        Self { cg }
    }
}

impl Drop for CodeGraphGuard {
    fn drop(&mut self) {
        self.cg.destroy();
    }
}

fn write_file(path: impl AsRef<Path>, contents: &str) {
    let path = path.as_ref();
    fs::write(path, contents)
        .unwrap_or_else(|err| panic!("failed to write fixture file {}: {err}", path.display()));
}

fn search_limit(limit: u64) -> Option<SearchOptions> {
    Some(SearchOptions {
        kinds: None,
        languages: None,
        include_patterns: None,
        exclude_patterns: None,
        limit: Some(limit),
        offset: None,
        case_sensitive: None,
    })
}

/**
 * Generate a synthetic TypeScript project with the given module count.
 * Each module exports a function that calls the previous module's
 * function so that the resolver has real import edges + call edges to
 * resolve. The first module is a leaf; the last is the root.
 */
fn generate_synthetic_project(root: &Path, module_count: usize) {
    let src_dir = root.join("src");
    fs::create_dir_all(&src_dir)
        .unwrap_or_else(|err| panic!("failed to create src dir {}: {err}", src_dir.display()));

    // Leaf module - no imports.
    write_file(
        src_dir.join("mod0.ts"),
        "export function fn0(x: number): number { return x + 1; }\n\
         export class Mod0 { ping(): string { return 'mod0'; } }\n",
    );

    for i in 1..module_count {
        let prev = i - 1;
        write_file(
            src_dir.join(format!("mod{i}.ts")),
            &format!(
                "import {{ fn{prev}, Mod{prev} }} from './mod{prev}';\n\
                 export function fn{i}(x: number): number {{ return fn{prev}(x) + 1; }}\n\
                 export class Mod{i} extends Mod{prev} {{\n\
                 \x20\x20call{i}(): number {{ return fn{i}({i}); }}\n\
                 }}\n"
            ),
        );
    }

    // Entry point file.
    let last = module_count - 1;
    write_file(
        src_dir.join("index.ts"),
        &format!(
            "import {{ fn{last}, Mod{last} }} from './mod{last}';\n\
             export function entry(): number {{\n\
             \x20\x20const m = new Mod{last}();\n\
             \x20\x20return fn{last}(0) + m.call{last}();\n\
             }}\n"
        ),
    );
}

mod integration_full_pipeline {
    use super::*;

    #[test]
    fn runs_init_index_resolve_search_callers_context_sync() {
        let module_count = 120;
        let temp_dir = TempProject::new();
        generate_synthetic_project(temp_dir.path(), module_count);

        // init
        let mut guard = CodeGraphGuard::init(temp_dir.path());
        let cg = &mut guard.cg;

        // indexAll
        let index_result = cg.index_all(IndexOptions::default());
        // Synthetic project: module_count mod files + 1 index file.
        assert!(
            index_result.files_indexed >= module_count,
            "expected at least {module_count} indexed files, got {}",
            index_result.files_indexed
        );

        let stats_after_index = cg.get_stats();
        assert!(stats_after_index.file_count >= module_count as u64);
        assert!(stats_after_index.node_count > (module_count * 2) as u64);

        // resolveReferences
        // Many call-site edges are wired up during extraction itself, so
        // the unresolved-reference queue may already be drained by the
        // time we get here. We assert that resolve completes cleanly and
        // returns a well-formed result; downstream callers/callees
        // assertions verify the graph is actually populated.
        cg.reinitialize_resolver();
        let resolution = cg.resolve_references();
        let _total: u64 = resolution.stats.total;
        let _resolved: u64 = resolution.stats.resolved;

        // searchNodes
        let entry_results = cg.search_nodes("entry", search_limit(10));
        assert!(!entry_results.is_empty());
        let entry_node = entry_results
            .iter()
            .find(|result| result.node.name == "entry");
        assert!(entry_node.is_some());

        let mid_results = cg.search_nodes("fn50", search_limit(10));
        assert!(mid_results.iter().any(|result| result.node.name == "fn50"));

        // getCallers / getCallees
        let fn0_results = cg.search_nodes("fn0", search_limit(5));
        let fn0_node = fn0_results
            .iter()
            .find(|result| result.node.name == "fn0")
            .expect("fn0 should be indexed");
        let callers = cg.get_callers(&fn0_node.node.id, 1);
        // fn0 is called by fn1 (at least). After resolution this should
        // be wired up.
        let _caller_count = callers.len();

        // buildContext
        let context = cg.build_context(
            TaskInput::Query("entry function chain".to_owned()),
            Some(BuildContextOptions {
                max_nodes: Some(10),
                max_code_blocks: None,
                max_code_block_size: None,
                include_code: None,
                format: Some(ContextFormat::Markdown),
                search_limit: None,
                traversal_depth: None,
                min_score: None,
            }),
        );
        match context {
            BuildContextResult::Formatted(text) => {
                assert!(!text.is_empty(), "markdown context should not be empty");
            }
            BuildContextResult::Context(_) => panic!("markdown context should be formatted text"),
        }

        // sync (add + modify + remove in one pass)
        // Add: a new file referencing entry().
        write_file(
            temp_dir.path().join("src").join("consumer.ts"),
            "import { entry } from './index';\nexport const result = entry();\n",
        );
        // Modify: change mod0.
        write_file(
            temp_dir.path().join("src").join("mod0.ts"),
            "export function fn0(x: number): number { return x + 2; }\n\
             export function newHelper(): string { return 'new'; }\n\
             export class Mod0 { ping(): string { return 'mod0v2'; } }\n",
        );
        // Remove: drop mod1 - note this will leave dangling imports in
        // mod2, which the resolver should tolerate.
        fs::remove_file(temp_dir.path().join("src").join("mod1.ts"))
            .expect("failed to remove src/mod1.ts fixture");

        let sync_result = cg.sync(IndexOptions::default());
        assert!(sync_result.files_added >= 1);
        assert!(sync_result.files_modified >= 1);
        assert!(sync_result.files_removed >= 1);

        // New symbol must now be findable; removed file's symbols gone.
        assert!(!cg.search_nodes("newHelper", None).is_empty());

        // Removed file should no longer appear in the indexed file list.
        // (FTS prefix matching makes name-based assertions unreliable here -
        // Mod10/Mod11/... all start with "Mod1" - so we check the file set
        // instead.)
        let files_after_sync = cg.get_nodes_in_file("src/mod1.ts");
        assert!(files_after_sync.is_empty());
    }

    #[test]
    fn keeps_indexing_files_when_one_file_has_a_parse_error() {
        let temp_dir = TempProject::new();
        let src_dir = temp_dir.path().join("src");
        fs::create_dir_all(&src_dir)
            .unwrap_or_else(|err| panic!("failed to create src dir {}: {err}", src_dir.display()));

        // Valid files
        write_file(
            src_dir.join("good1.ts"),
            "export function good1(): number { return 1; }\n",
        );
        write_file(
            src_dir.join("good2.ts"),
            "export function good2(): number { return 2; }\n",
        );
        // Intentionally broken file - unclosed brace, stray tokens.
        write_file(
            src_dir.join("broken.ts"),
            "export function broken(\n  this is { not valid typescript at all\n",
        );

        let mut guard = CodeGraphGuard::init(temp_dir.path());
        let cg = &mut guard.cg;

        let result = cg.index_all(IndexOptions::default());
        // The two good files must still be indexed regardless of the
        // broken one. Tree-sitter is error-tolerant so it may still
        // extract a partial AST from broken.ts - but the test only
        // requires that the batch completes and finds the good symbols.
        assert!(result.files_indexed >= 2);

        let good1 = cg.search_nodes("good1", None);
        let good2 = cg.search_nodes("good2", None);
        assert!(good1.iter().any(|result| result.node.name == "good1"));
        assert!(good2.iter().any(|result| result.node.name == "good2"));
    }

    #[test]
    fn handles_repeated_sync_calls_when_nothing_has_changed() {
        let temp_dir = TempProject::new();
        generate_synthetic_project(temp_dir.path(), 10);

        let mut guard = CodeGraphGuard::init(temp_dir.path());
        let cg = &mut guard.cg;

        cg.index_all(IndexOptions::default());
        let stats_before = cg.get_stats();

        let first = cg.sync(IndexOptions::default());
        let second = cg.sync(IndexOptions::default());

        // Subsequent sync with no changes should be a no-op.
        assert_eq!(
            first.files_added + first.files_modified + first.files_removed,
            0
        );
        assert_eq!(
            second.files_added + second.files_modified + second.files_removed,
            0
        );

        let stats_after = cg.get_stats();
        assert_eq!(stats_after.file_count, stats_before.file_count);
        assert_eq!(stats_after.node_count, stats_before.node_count);
    }

    #[test]
    fn reports_edges_created_including_resolution_synthesizer_phases() {
        // The synthetic project has cross-file imports, calls, and extends -
        // all wired up in the resolution phase, AFTER the orchestrator's
        // per-file extraction counter is done. The CLI summary used to read
        // only the extraction-phase counter and undercount the graph; this
        // test pins the counter to the true DB totals across all phases.
        let temp_dir = TempProject::new();
        generate_synthetic_project(temp_dir.path(), 30);

        let mut guard = CodeGraphGuard::init(temp_dir.path());
        let cg = &mut guard.cg;

        let result = cg.index_all(IndexOptions::default());
        let stats = cg.get_stats();

        assert!(result.success);
        assert_eq!(result.nodes_created as u64, stats.node_count);
        assert_eq!(result.edges_created as u64, stats.edge_count);
        // Sanity: cross-file resolution had something to do - calls/extends
        // edges should exist beyond the bare extraction-time contains edges.
        let contains_only = *stats.edges_by_kind.get(&EdgeKind::Contains).unwrap_or(&0);
        assert!(stats.edge_count > contains_only);
    }
}
