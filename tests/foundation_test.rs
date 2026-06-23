//! Foundation tests.
//!
//! This is the Rust port of `__tests__/foundation.test.ts`.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use rustcodegraph::directory::{
    code_graph_dir_name, get_code_graph_dir, is_code_graph_data_dir, validate_directory,
};
use rustcodegraph::{
    CodeGraph, CodeGraphError, DatabaseConnection, InitOptions, get_database_path,
};

static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);
static RUSTCODEGRAPH_DIR_ENV_MUTEX: Mutex<()> = Mutex::new(());

struct CodeGraphDirEnvGuard {
    _lock: MutexGuard<'static, ()>,
    saved_rustcodegraph_dir: Option<String>,
}

impl CodeGraphDirEnvGuard {
    fn new() -> Self {
        let lock = RUSTCODEGRAPH_DIR_ENV_MUTEX
            .lock()
            .expect("RUSTCODEGRAPH_DIR env mutex should not be poisoned");
        let saved_rustcodegraph_dir = env::var("RUSTCODEGRAPH_DIR").ok();
        unsafe {
            env::remove_var("RUSTCODEGRAPH_DIR");
        }
        Self {
            _lock: lock,
            saved_rustcodegraph_dir,
        }
    }

    fn unset(&self) {
        unsafe {
            env::remove_var("RUSTCODEGRAPH_DIR");
        }
    }

    fn set(&self, value: &str) {
        unsafe {
            env::set_var("RUSTCODEGRAPH_DIR", value);
        }
    }

    fn set_rust(&self, value: &str) {
        unsafe {
            env::set_var("RUSTCODEGRAPH_DIR", value);
        }
    }
}

impl Drop for CodeGraphDirEnvGuard {
    fn drop(&mut self) {
        unsafe {
            if let Some(value) = &self.saved_rustcodegraph_dir {
                env::set_var("RUSTCODEGRAPH_DIR", value);
            } else {
                env::remove_var("RUSTCODEGRAPH_DIR");
            }
        }
    }
}

struct TempDir {
    path: PathBuf,
    env_guard: CodeGraphDirEnvGuard,
}

impl TempDir {
    fn new(prefix: &str) -> Self {
        for _ in 0..100 {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time should be after the Unix epoch")
                .as_nanos();
            let suffix = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
            let path =
                env::temp_dir().join(format!("{prefix}-{}-{unique}-{suffix}", std::process::id()));
            match fs::create_dir(&path) {
                Ok(()) => {
                    return Self {
                        path,
                        env_guard: CodeGraphDirEnvGuard::new(),
                    };
                }
                Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(err) => panic!("failed to create temp dir {}: {err}", path.display()),
            }
        }
        panic!("failed to create unique temp dir with prefix {prefix}");
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn unset_codegraph_dir(&self) {
        self.env_guard.unset();
    }

    fn set_codegraph_dir(&self, value: &str) {
        self.env_guard.set(value);
    }

    fn set_rustcodegraph_dir(&self, value: &str) {
        self.env_guard.set_rust(value);
    }

    fn write(&self, relative_path: &str, content: &str) {
        let path = self.path.join(relative_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .unwrap_or_else(|err| panic!("failed to create {}: {err}", parent.display()));
        }
        fs::write(&path, content)
            .unwrap_or_else(|err| panic!("failed to write {}: {err}", path.display()));
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

    fn get_mut(&mut self) -> &mut CodeGraph {
        self.value.as_mut().expect("CodeGraph should be present")
    }

    fn close(&mut self) {
        if let Some(mut value) = self.value.take() {
            value.close();
        }
    }

    fn uninitialize(&mut self) {
        if let Some(mut value) = self.value.take() {
            value
                .uninitialize()
                .expect("CodeGraph should uninitialize cleanly");
        }
    }
}

impl Drop for CodeGraphGuard {
    fn drop(&mut self) {
        self.close();
    }
}

struct DatabaseGuard {
    value: Option<DatabaseConnection>,
}

impl DatabaseGuard {
    fn new(value: DatabaseConnection) -> Self {
        Self { value: Some(value) }
    }

    fn get(&self) -> &DatabaseConnection {
        self.value
            .as_ref()
            .expect("DatabaseConnection should be present")
    }

    fn get_mut(&mut self) -> &mut DatabaseConnection {
        self.value
            .as_mut()
            .expect("DatabaseConnection should be present")
    }

    fn close(&mut self) {
        if let Some(mut value) = self.value.take() {
            value.close().expect("database should close cleanly");
        }
    }
}

impl Drop for DatabaseGuard {
    fn drop(&mut self) {
        self.close();
    }
}

struct InitializedProject {
    cg: CodeGraphGuard,
    _temp: TempDir,
}

impl InitializedProject {
    fn new(prefix: &str) -> Self {
        let temp = TempDir::new(prefix);
        let cg = CodeGraph::init_sync(temp.path()).expect("CodeGraph should initialize");
        Self {
            cg: CodeGraphGuard::new(cg),
            _temp: temp,
        }
    }
}

fn assert_codegraph_error_contains(result: Result<CodeGraph, CodeGraphError>, needle: &str) {
    match result {
        Ok(_) => panic!("expected CodeGraph error containing {needle:?}"),
        Err(err) => {
            let message = err.to_string().to_lowercase();
            assert!(
                message.contains(needle),
                "expected {message:?} to contain {needle:?}"
            );
        }
    }
}

fn assert_database_error_contains<T, E: std::fmt::Display>(result: Result<T, E>, needle: &str) {
    match result {
        Ok(_) => panic!("expected database error containing {needle:?}"),
        Err(err) => {
            let message = err.to_string().to_lowercase();
            assert!(
                message.contains(needle),
                "expected {message:?} to contain {needle:?}"
            );
        }
    }
}

fn assert_invalid_codegraph_dir_falls_back(bad: &str) {
    let temp = TempDir::new("codegraph-dirname");
    temp.set_codegraph_dir(bad);
    assert_eq!(code_graph_dir_name(), ".rustcodegraph");
}

mod codegraph_foundation {
    use super::*;

    mod initialization {
        use super::*;

        #[test]
        fn should_initialize_a_new_project() {
            let temp = TempDir::new("codegraph-test");
            let mut cg = CodeGraphGuard::new(
                CodeGraph::init_sync(temp.path()).expect("CodeGraph should initialize"),
            );

            assert!(CodeGraph::is_initialized(temp.path()));
            assert!(get_code_graph_dir(temp.path()).exists());
            assert!(get_database_path(temp.path()).exists());

            cg.close();
        }

        #[test]
        fn should_create_gitignore_in_codegraph_directory() {
            let temp = TempDir::new("codegraph-test");
            let mut cg = CodeGraphGuard::new(
                CodeGraph::init_sync(temp.path()).expect("CodeGraph should initialize"),
            );

            let gitignore_path = get_code_graph_dir(temp.path()).join(".gitignore");
            assert!(gitignore_path.exists());

            let content =
                fs::read_to_string(&gitignore_path).expect(".rustcodegraph/.gitignore should read");
            // Ignore everything in .rustcodegraph/ except this file itself, so transient
            // files (db, daemon.pid, sockets, logs) never show up in git. (#492, #484)
            assert!(content.contains('*'));
            assert!(content.contains("!.gitignore"));

            cg.close();
        }

        #[test]
        fn should_throw_if_already_initialized() {
            let temp = TempDir::new("codegraph-test");
            let mut cg = CodeGraphGuard::new(
                CodeGraph::init_sync(temp.path()).expect("CodeGraph should initialize"),
            );
            cg.close();

            assert_codegraph_error_contains(
                CodeGraph::init_sync(temp.path()),
                "already initialized",
            );
        }
    }

    mod opening_projects {
        use super::*;

        #[test]
        fn should_open_an_existing_project() {
            let temp = TempDir::new("codegraph-test");
            let mut cg1 = CodeGraphGuard::new(
                CodeGraph::init_sync(temp.path()).expect("CodeGraph should initialize"),
            );
            cg1.close();

            let mut cg2 = CodeGraphGuard::new(
                CodeGraph::open_sync(temp.path()).expect("project should open"),
            );
            assert_eq!(PathBuf::from(cg2.get().get_project_root()), temp.path());
            cg2.close();
        }

        #[test]
        fn should_throw_if_not_initialized() {
            let temp = TempDir::new("codegraph-test");
            assert_codegraph_error_contains(CodeGraph::open_sync(temp.path()), "not initialized");
        }
    }

    mod static_methods {
        use super::*;

        #[test]
        fn is_initialized_should_return_false_for_new_directory() {
            let temp = TempDir::new("codegraph-test");
            assert!(!CodeGraph::is_initialized(temp.path()));
        }

        #[test]
        fn is_initialized_should_return_true_after_init() {
            let temp = TempDir::new("codegraph-test");
            let mut cg = CodeGraphGuard::new(
                CodeGraph::init_sync(temp.path()).expect("CodeGraph should initialize"),
            );
            assert!(CodeGraph::is_initialized(temp.path()));
            cg.close();
        }
    }

    mod database {
        use super::*;

        #[test]
        fn should_create_database_with_correct_schema() {
            let mut fixture = InitializedProject::new("codegraph-test");

            let stats = fixture.cg.get().get_stats();
            assert_eq!(stats.node_count, 0);
            assert_eq!(stats.edge_count, 0);
            assert_eq!(stats.file_count, 0);

            fixture.cg.close();
        }

        #[test]
        fn should_return_correct_database_size() {
            let mut fixture = InitializedProject::new("codegraph-test");
            let stats = fixture.cg.get().get_stats();

            assert!(stats.db_size_bytes > 0);

            fixture.cg.close();
        }

        #[test]
        fn should_support_optimize_operation() {
            let mut fixture = InitializedProject::new("codegraph-test");

            fixture.cg.get_mut().optimize();

            fixture.cg.close();
        }

        #[test]
        fn should_support_clear_operation() {
            let mut fixture = InitializedProject::new("codegraph-test");

            fixture.cg.get_mut().clear();

            let stats = fixture.cg.get().get_stats();
            assert_eq!(stats.node_count, 0);

            fixture.cg.close();
        }
    }

    mod directory_management {
        use super::*;

        #[test]
        fn should_validate_directory_structure() {
            let temp = TempDir::new("codegraph-test");
            let mut cg = CodeGraphGuard::new(
                CodeGraph::init_sync(temp.path()).expect("CodeGraph should initialize"),
            );
            cg.close();

            let validation = validate_directory(temp.path());
            assert!(validation.valid);
            assert!(validation.errors.is_empty());
        }

        #[test]
        fn should_detect_invalid_directory() {
            let temp = TempDir::new("codegraph-test");
            let validation = validate_directory(temp.path());
            assert!(!validation.valid);
            assert!(!validation.errors.is_empty());
        }

        #[test]
        fn leaves_legacy_codegraph_gitignore_marker_untouched() {
            let temp = TempDir::new("codegraph-test");
            let mut cg = CodeGraphGuard::new(
                CodeGraph::init_sync(temp.path()).expect("CodeGraph should initialize"),
            );
            cg.close();

            let gitignore_path = get_code_graph_dir(temp.path()).join(".gitignore");
            // RustCodeGraph no longer treats the old CodeGraph marker as its own
            // marker; a leftover legacy file is user-owned state, not a migration
            // target.
            let legacy = "# CodeGraph data files\n\
# These are local to each machine and should not be committed\n\n\
# Database\n*.db\n*.db-wal\n*.db-shm\n\n\
# Cache\ncache/\n\n# Logs\n*.log\n\n# Hook markers\n.dirty\n";
            fs::write(&gitignore_path, legacy)
                .expect("legacy .rustcodegraph/.gitignore should be written");

            let mut cg2 = CodeGraphGuard::new(
                CodeGraph::open_sync(temp.path()).expect("project should open"),
            );
            cg2.close();

            assert_eq!(
                fs::read_to_string(&gitignore_path).expect("legacy .gitignore should read"),
                legacy
            );
        }

        #[test]
        fn leaves_a_user_customized_codegraph_gitignore_untouched() {
            let temp = TempDir::new("codegraph-test");
            let mut cg = CodeGraphGuard::new(
                CodeGraph::init_sync(temp.path()).expect("CodeGraph should initialize"),
            );
            cg.close();

            let gitignore_path = get_code_graph_dir(temp.path()).join(".gitignore");
            // No CodeGraph header -> user-authored -> must not be rewritten.
            let custom = "# my own rules\n*.db\n!keep-this.json\n";
            fs::write(&gitignore_path, custom)
                .expect("custom .rustcodegraph/.gitignore should be written");

            let mut cg2 = CodeGraphGuard::new(
                CodeGraph::open_sync(temp.path()).expect("project should open"),
            );
            cg2.close();

            assert_eq!(
                fs::read_to_string(&gitignore_path).expect("custom .gitignore should read"),
                custom
            );
        }
    }

    mod uninitialize {
        use super::*;

        #[test]
        fn should_remove_codegraph_directory() {
            let temp = TempDir::new("codegraph-test");
            let mut cg = CodeGraphGuard::new(
                CodeGraph::init_sync(temp.path()).expect("CodeGraph should initialize"),
            );

            cg.uninitialize();

            assert!(!get_code_graph_dir(temp.path()).exists());
            assert!(!CodeGraph::is_initialized(temp.path()));
        }
    }

    mod close_destroy {
        use super::*;

        #[test]
        fn should_close_database_but_keep_codegraph_directory() {
            let temp = TempDir::new("codegraph-test");
            let mut cg = CodeGraphGuard::new(
                CodeGraph::init_sync(temp.path()).expect("CodeGraph should initialize"),
            );

            cg.get_mut().destroy();

            assert!(get_code_graph_dir(temp.path()).exists());
            assert!(CodeGraph::is_initialized(temp.path()));
        }
    }

    mod graph_query_methods {
        use super::*;

        #[test]
        fn should_throw_node_not_found_for_non_existent_nodes() {
            let mut fixture = InitializedProject::new("codegraph-test");

            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _ = fixture.cg.get_mut().get_context("non-existent");
            }));
            assert!(result.is_err());

            fixture.cg.close();
        }

        #[test]
        fn should_return_empty_results_for_non_existent_nodes() {
            let mut fixture = InitializedProject::new("codegraph-test");

            let traverse_result = fixture.cg.get_mut().traverse("non-existent", None);
            assert!(traverse_result.nodes.is_empty());

            let call_graph = fixture.cg.get_mut().get_call_graph("non-existent", 2);
            assert!(call_graph.nodes.is_empty());

            let type_hierarchy = fixture.cg.get_mut().get_type_hierarchy("non-existent");
            assert!(type_hierarchy.nodes.is_empty());

            let impact = fixture.cg.get_mut().get_impact_radius("non-existent", 3);
            assert!(impact.nodes.is_empty());

            let ancestors = fixture.cg.get_mut().get_ancestors("non-existent");
            assert!(ancestors.is_empty());

            let children = fixture.cg.get_mut().get_children("non-existent");
            assert!(children.is_empty());

            let dependencies = fixture
                .cg
                .get_mut()
                .get_file_dependencies("non-existent.ts");
            assert!(dependencies.is_empty());

            let dependents = fixture.cg.get_mut().get_file_dependents("non-existent.ts");
            assert!(dependents.is_empty());

            let cycles = fixture.cg.get_mut().find_circular_dependencies();
            assert!(cycles.is_empty());

            let usages = fixture.cg.get_mut().find_usages("non-existent");
            assert!(usages.is_empty());

            fixture.cg.close();
        }

        #[test]
        fn reference_resolution_facade_methods_are_callable() {
            let mut fixture = InitializedProject::new("codegraph-test");

            let resolved: rustcodegraph::ResolutionResult =
                fixture.cg.get_mut().resolve_references();
            assert_eq!(resolved.stats.total, 0);
            assert!(resolved.resolved.is_empty());
            assert!(resolved.unresolved.is_empty());

            let batched = fixture.cg.get_mut().resolve_references_batched();
            assert_eq!(batched.stats.total, 0);
            assert!(fixture.cg.get_mut().get_detected_frameworks().is_empty());

            fixture.cg.get_mut().reinitialize_resolver();
            assert!(fixture.cg.get().get_project_name_tokens().is_empty());
            assert!(fixture.cg.get().get_top_route_file().is_none());
            assert!(fixture.cg.get().get_routing_manifest(None).is_none());

            fixture.cg.close();
        }
    }
}

mod public_api_surface {
    use super::*;

    #[test]
    fn crate_root_reexports_sdk_building_blocks() {
        let temp = TempDir::new("codegraph-public-api");
        let db_path = rustcodegraph::get_database_path(temp.path());
        let mut db = DatabaseGuard::new(
            rustcodegraph::DatabaseConnection::initialize(&db_path)
                .expect("database should initialize from the crate root export"),
        );

        let mut queries = rustcodegraph::QueryBuilder::new(db.get_mut().get_db());
        let stats = queries.get_stats().expect("stats query should run");
        assert_eq!(stats.node_count, 0);

        assert_eq!(
            rustcodegraph::detect_language("src/lib.rs", None),
            rustcodegraph::Language::Rust
        );
        assert!(rustcodegraph::is_language_supported(
            rustcodegraph::Language::Rust
        ));
        let _ = rustcodegraph::is_grammar_loaded(rustcodegraph::Language::Rust);
        assert!(rustcodegraph::get_supported_languages().contains(&rustcodegraph::Language::Rust));

        let _init_grammars = rustcodegraph::init_grammars;
        let _load_grammars_for_languages = rustcodegraph::load_grammars_for_languages;
        let _load_all_grammars = rustcodegraph::load_all_grammars;
        let _resolution_result = rustcodegraph::ResolutionResult::default();
        let _file_watcher_options = rustcodegraph::FileWatcherOptions::default();
        let _file_watcher_type = std::any::type_name::<rustcodegraph::FileWatcher>();
        let _file_watcher_pending_type =
            std::any::type_name::<rustcodegraph::FileWatcherPendingFile>();

        let lock_error = rustcodegraph::LockUnavailableError::default();
        assert!(lock_error.to_string().contains("lock"));

        let mut server = rustcodegraph::MCPServer::new(None);
        assert_eq!(format!("{:?}", server.mode()), "Unstarted");
        server.stop();

        assert_eq!(db_path, get_database_path(temp.path()));
        let _rustcodegraph_dir_export =
            std::any::type_name_of_val(&rustcodegraph::RUSTCODEGRAPH_DIR);
    }
}

mod database_connection {
    use super::*;

    #[test]
    fn should_initialize_new_database() {
        let temp = TempDir::new("codegraph-test");
        let db_path = temp.path().join("test.db");
        let mut db = DatabaseGuard::new(
            DatabaseConnection::initialize(&db_path).expect("database should initialize"),
        );

        assert!(db.get().is_open());
        assert!(db_path.exists());

        db.close();
    }

    #[test]
    fn should_get_schema_version() {
        let temp = TempDir::new("codegraph-test");
        let db_path = temp.path().join("test.db");
        let mut db = DatabaseGuard::new(
            DatabaseConnection::initialize(&db_path).expect("database should initialize"),
        );

        let version = db
            .get_mut()
            .get_schema_version()
            .expect("schema version query should succeed");
        assert!(version.is_some());
        assert_eq!(version.expect("schema version should exist").version, 5);

        db.close();
    }

    #[test]
    fn should_support_transactions() {
        let temp = TempDir::new("codegraph-test");
        let db_path = temp.path().join("test.db");
        let mut db = DatabaseGuard::new(
            DatabaseConnection::initialize(&db_path).expect("database should initialize"),
        );

        let result = db
            .get_mut()
            .transaction(|_| Ok(42))
            .expect("transaction should return callback value");

        assert_eq!(result, 42);

        db.close();
    }

    #[test]
    fn should_throw_when_opening_non_existent_database() {
        let temp = TempDir::new("codegraph-test");
        let db_path = temp.path().join("nonexistent.db");

        assert_database_error_contains(DatabaseConnection::open(&db_path), "not found");
    }
}

mod query_builder {
    use super::*;

    #[test]
    fn should_return_null_for_non_existent_node() {
        let mut fixture = InitializedProject::new("codegraph-test");
        let node = fixture.cg.get_mut().get_node("nonexistent");
        assert!(node.is_none());
    }

    #[test]
    fn should_return_empty_array_for_nodes_in_non_existent_file() {
        let mut fixture = InitializedProject::new("codegraph-test");
        let nodes = fixture.cg.get_mut().get_nodes_in_file("nonexistent.ts");
        assert!(nodes.is_empty());
    }

    #[test]
    fn should_return_empty_array_for_edges_from_non_existent_node() {
        let mut fixture = InitializedProject::new("codegraph-test");
        let edges = fixture.cg.get_mut().get_outgoing_edges("nonexistent");
        assert!(edges.is_empty());
    }

    #[test]
    fn should_return_null_for_non_existent_file() {
        let mut fixture = InitializedProject::new("codegraph-test");
        let file = fixture.cg.get_mut().get_file("nonexistent.ts");
        assert!(file.is_none());
    }

    #[test]
    fn should_return_empty_array_for_files_when_none_tracked() {
        let mut fixture = InitializedProject::new("codegraph-test");
        let files = fixture.cg.get_mut().get_files();
        assert!(files.is_empty());
    }
}

// Two environments that share one working tree (Windows-native + WSL) must not
// share one `.rustcodegraph/`. RUSTCODEGRAPH_DIR overrides the data directory name so
// each side keeps its own index in the same tree (issue #636).
mod codegraph_dir_override_636 {
    use super::*;

    mod code_graph_dir_name {
        use super::*;

        #[test]
        fn defaults_to_codegraph_when_unset() {
            let temp = TempDir::new("codegraph-dirname");
            temp.unset_codegraph_dir();
            assert_eq!(code_graph_dir_name(), ".rustcodegraph");
        }

        #[test]
        fn honors_a_valid_override() {
            let temp = TempDir::new("codegraph-dirname");
            temp.set_codegraph_dir(".rustcodegraph-win");
            assert_eq!(code_graph_dir_name(), ".rustcodegraph-win");
        }

        #[test]
        fn honors_the_rustcodegraph_override_name() {
            let temp = TempDir::new("codegraph-dirname");
            temp.set_rustcodegraph_dir(".rustcodegraph-wsl");
            assert_eq!(code_graph_dir_name(), ".rustcodegraph-wsl");
        }

        // Anything that isn't a plain segment could escape the project root or
        // clobber it, so it's ignored in favor of the default.
        #[test]
        fn falls_back_to_codegraph_for_invalid_value_foo_slash_bar() {
            assert_invalid_codegraph_dir_falls_back("foo/bar");
        }

        #[test]
        fn falls_back_to_codegraph_for_invalid_value_a_backslash_b() {
            assert_invalid_codegraph_dir_falls_back("a\\b");
        }

        #[test]
        fn falls_back_to_codegraph_for_invalid_value_dot_dot() {
            assert_invalid_codegraph_dir_falls_back("..");
        }

        #[test]
        fn falls_back_to_codegraph_for_invalid_value_dot_dot_slash_x() {
            assert_invalid_codegraph_dir_falls_back("../x");
        }

        #[test]
        fn falls_back_to_codegraph_for_invalid_value_dot() {
            assert_invalid_codegraph_dir_falls_back(".");
        }

        #[test]
        fn falls_back_to_codegraph_for_invalid_value_abs_path() {
            assert_invalid_codegraph_dir_falls_back("/abs/path");
        }

        #[test]
        fn falls_back_to_codegraph_for_invalid_value_spaces() {
            assert_invalid_codegraph_dir_falls_back("   ");
        }

        #[test]
        fn falls_back_to_codegraph_for_invalid_value_empty() {
            assert_invalid_codegraph_dir_falls_back("");
        }
    }

    mod is_code_graph_data_dir {
        use super::*;

        #[test]
        fn matches_the_default_the_active_override_and_rustcodegraph_siblings() {
            let temp = TempDir::new("codegraph-dirname");
            temp.set_codegraph_dir(".rustcodegraph-win");
            assert!(is_code_graph_data_dir(".rustcodegraph"));
            assert!(is_code_graph_data_dir(".rustcodegraph-win"));
            assert!(is_code_graph_data_dir(".rustcodegraph-wsl"));
            assert!(!is_code_graph_data_dir(".codegraph"));
            assert!(!is_code_graph_data_dir(".codegraph-win"));
        }

        #[test]
        fn does_not_match_unrelated_directories() {
            let temp = TempDir::new("codegraph-dirname");
            temp.unset_codegraph_dir();
            for name in [
                "src",
                "node_modules",
                ".git",
                "codegraph",
                ".rustcodegraphextra",
            ] {
                assert!(
                    !is_code_graph_data_dir(name),
                    "{name:?} should not be treated as a CodeGraph data dir"
                );
            }
        }
    }

    #[test]
    fn init_writes_the_index_under_the_overridden_directory_not_codegraph() {
        let temp = TempDir::new("codegraph-dirname");
        temp.set_codegraph_dir(".rustcodegraph-win");

        let mut cg = CodeGraphGuard::new(
            CodeGraph::init_sync(temp.path()).expect("CodeGraph should initialize"),
        );
        assert!(
            temp.path()
                .join(".rustcodegraph-win/rustcodegraph.db")
                .exists()
        );
        assert!(!temp.path().join(".rustcodegraph").exists());
        assert_eq!(
            get_code_graph_dir(temp.path()),
            temp.path().join(".rustcodegraph-win")
        );
        assert!(CodeGraph::is_initialized(temp.path()));
        cg.close();
    }

    #[test]
    fn two_index_dirs_coexist_in_one_tree_and_the_override_side_skips_the_sibling() {
        let temp = TempDir::new("codegraph-dirname");

        // WSL side: default `.rustcodegraph`, with a source file.
        temp.unset_codegraph_dir();
        temp.write("app.ts", "export function onlyReal() {}\n");
        let mut wsl = CodeGraphGuard::new(
            CodeGraph::init(temp.path(), InitOptions { index: true })
                .expect("default-side CodeGraph should initialize"),
        );
        wsl.close();

        // Windows side: override dir, same tree. Plant a decoy source file INSIDE
        // the WSL data dir - the override-side index must not pick it up.
        temp.set_codegraph_dir(".rustcodegraph-win");
        temp.write(
            ".rustcodegraph/decoy.ts",
            "export function decoyLeak() {}\n",
        );
        let mut win = CodeGraphGuard::new(
            CodeGraph::init(temp.path(), InitOptions { index: true })
                .expect("override-side CodeGraph should initialize"),
        );

        assert!(temp.path().join(".rustcodegraph/rustcodegraph.db").exists());
        assert!(
            temp.path()
                .join(".rustcodegraph-win/rustcodegraph.db")
                .exists()
        );
        assert!(!win.get_mut().search_nodes("onlyReal", None).is_empty());
        assert!(win.get_mut().search_nodes("decoyLeak", None).is_empty());

        win.close();
    }
}
