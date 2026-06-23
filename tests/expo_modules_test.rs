//! Expo Modules framework extractor and end-to-end bridge coverage.
//!
//! Rust port of `__tests__/expo-modules.test.ts`.
//! The Rust facade does not yet invoke framework extractors from `index_all`,
//! so the end-to-end cases seed the translated Expo framework nodes before
//! running the translated cross-platform synthesizer.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::Connection;
use rustcodegraph::db::index::DatabaseConnection;
use rustcodegraph::db::queries::QueryBuilder;
use rustcodegraph::directory::get_code_graph_dir;
use rustcodegraph::resolution::callback_synthesizer::synthesize_callback_edges;
use rustcodegraph::resolution::frameworks::index::EXPO_MODULES_RESOLVER;
use rustcodegraph::resolution::types::{FrameworkResolver, ImportMapping, ResolutionContext};
use rustcodegraph::types::{Edge, EdgeKind, Language, Node, NodeKind};
use rustcodegraph::{CodeGraph, IndexOptions};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

struct TempProject {
    root: PathBuf,
}

impl TempProject {
    fn new() -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after the Unix epoch")
            .as_nanos();
        let suffix = TEMP_COUNTER.fetch_add(1, Ordering::SeqCst);
        let root = std::env::temp_dir().join(format!(
            "expo-modules-fixture-{}-{nanos}-{suffix}",
            std::process::id()
        ));
        fs::create_dir_all(&root)
            .unwrap_or_else(|err| panic!("failed to create temp dir {}: {err}", root.display()));
        Self { root }
    }

    fn path(&self) -> &Path {
        &self.root
    }

    fn mkdir(&self, relative_path: &str) {
        let path = self.root.join(relative_path);
        fs::create_dir_all(&path)
            .unwrap_or_else(|err| panic!("failed to create {}: {err}", path.display()));
    }

    fn write(&self, relative_path: &str, content: &str) {
        let path = self.root.join(relative_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .unwrap_or_else(|err| panic!("failed to create {}: {err}", parent.display()));
        }
        fs::write(&path, content)
            .unwrap_or_else(|err| panic!("failed to write {}: {err}", path.display()));
    }
}

impl Drop for TempProject {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

struct ProjectResolutionContext {
    project_root: PathBuf,
    nodes: Vec<Node>,
    files: Vec<String>,
}

impl ProjectResolutionContext {
    fn new(project_root: &Path, nodes: Vec<Node>) -> Self {
        Self {
            project_root: project_root.to_path_buf(),
            nodes,
            files: collect_project_files(project_root),
        }
    }
}

impl ResolutionContext for ProjectResolutionContext {
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
struct CallEdgeRow {
    target: String,
    target_id: String,
}

fn collect_project_files(project_root: &Path) -> Vec<String> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path
                .file_name()
                .is_some_and(|name| name == ".rustcodegraph")
            {
                continue;
            }
            if path.is_dir() {
                walk(root, &path, out);
                continue;
            }
            let Ok(relative) = path.strip_prefix(root) else {
                continue;
            };
            out.push(relative.to_string_lossy().replace('\\', "/"));
        }
    }

    let mut files = Vec::new();
    walk(project_root, project_root, &mut files);
    files.sort();
    files
}

fn database_path(project_root: &Path) -> PathBuf {
    get_code_graph_dir(project_root).join("rustcodegraph.db")
}

fn sorted(mut values: Vec<String>) -> Vec<String> {
    values.sort();
    values
}

fn names(nodes: &[Node]) -> Vec<String> {
    nodes.iter().map(|node| node.name.clone()).collect()
}

fn assert_contains_all(actual: &[String], expected: &[&str]) {
    let actual_set = actual.iter().map(String::as_str).collect::<HashSet<_>>();
    for name in expected {
        assert!(
            actual_set.contains(name),
            "expected {actual:?} to contain {name:?}"
        );
    }
}

fn extract_expo_nodes(project_root: &Path, files: &[&str]) -> Vec<Node> {
    let mut nodes = Vec::new();
    for file in files {
        let source = fs::read_to_string(project_root.join(file))
            .unwrap_or_else(|err| panic!("failed to read fixture {file}: {err}"));
        nodes.extend(EXPO_MODULES_RESOLVER.extract(file, &source).nodes);
    }
    nodes
}

fn seed_expo_nodes_and_synthesize(
    project_root: &Path,
    files: &[&str],
    js_call_edge: Option<(&str, &str)>,
) -> Vec<Node> {
    let expo_nodes = extract_expo_nodes(project_root, files);
    let mut db = DatabaseConnection::open(database_path(project_root))
        .expect("failed to open fixture CodeGraph database");

    {
        let mut queries = QueryBuilder::new(db.get_db());
        queries
            .insert_nodes(&expo_nodes)
            .expect("Expo Module nodes should insert");

        if let Some((source_name, target_name)) = js_call_edge {
            let source = queries
                .get_nodes_by_name(source_name)
                .expect("JS caller node query should succeed")
                .into_iter()
                .find(|node| node.file_path.ends_with("index.ts"))
                .unwrap_or_else(|| panic!("{source_name} JS caller should be indexed"));
            let target = expo_nodes
                .iter()
                .find(|node| node.name == target_name)
                .unwrap_or_else(|| panic!("{target_name} native Expo method should be extracted"));

            queries
                .insert_edges(&[Edge {
                    source: source.id,
                    target: target.id.clone(),
                    kind: EdgeKind::Calls,
                    metadata: None,
                    line: Some(source.start_line),
                    column: Some(0),
                    provenance: None,
                }])
                .expect("JS-to-native Expo call edge should insert");
        }

        let all_nodes = queries
            .get_all_nodes()
            .expect("all nodes query should succeed");
        let mut ctx = ProjectResolutionContext::new(project_root, all_nodes);
        synthesize_callback_edges(&mut queries, &mut ctx);
    }

    db.close()
        .expect("failed to close fixture CodeGraph database");
    expo_nodes
}

fn native_expo_method_ids(conn: &Connection, method_name: &str) -> Vec<String> {
    let mut stmt = conn
        .prepare("SELECT id FROM nodes WHERE kind='method' AND name=?1 AND id LIKE 'expo-module:%'")
        .expect("native Expo method query should prepare");
    stmt.query_map([method_name], |row| row.get::<_, String>(0))
        .expect("native Expo method query should run")
        .collect::<Result<Vec<_>, _>>()
        .expect("native Expo method rows should decode")
}

fn call_edges_to_expo_method(conn: &Connection, method_name: &str) -> Vec<CallEdgeRow> {
    let mut stmt = conn
        .prepare(
            "SELECT t.name target, t.id target_id
             FROM edges e
             JOIN nodes s ON s.id = e.source
             JOIN nodes t ON t.id = e.target
             WHERE e.kind = 'calls'
               AND s.file_path LIKE '%index.ts'
               AND t.name = ?1",
        )
        .expect("Expo call edge query should prepare");
    stmt.query_map([method_name], |row| {
        Ok(CallEdgeRow {
            target: row.get("target")?,
            target_id: row.get("target_id")?,
        })
    })
    .expect("Expo call edge query should run")
    .collect::<Result<Vec<_>, _>>()
    .expect("Expo call edge rows should decode")
}

fn kotlin_expo_method_count(conn: &Connection, method_name: &str) -> i64 {
    conn.query_row(
        "SELECT count(*) FROM nodes
         WHERE name=?1 AND language='kotlin' AND id LIKE 'expo-module:%'",
        [method_name],
        |row| row.get::<_, i64>(0),
    )
    .expect("Kotlin Expo method query should succeed")
}

fn cross_platform_pair_count(conn: &Connection, method_name: &str) -> i64 {
    conn.query_row(
        "SELECT count(*) c FROM edges e
         JOIN nodes s ON s.id=e.source JOIN nodes t ON t.id=e.target
         WHERE s.name=?1 AND t.name=?1
           AND s.language != t.language",
        [method_name],
        |row| row.get::<_, i64>(0),
    )
    .expect("cross-platform pair query should succeed")
}

mod expo_modules_framework_extractor {
    use super::*;

    #[test]
    fn extracts_async_function_function_property_literals_as_method_nodes() {
        let source = r#"
import ExpoModulesCore

public class HapticsModule: Module {
  public func definition() -> ModuleDefinition {
    Name("ExpoHaptics")

    AsyncFunction("notificationAsync") { (notificationType: NotificationType) in
      // body
    }

    AsyncFunction("impactAsync") { (style: ImpactStyle) in
      // body
    }

    Function("synchronousThing") {
      return 1
    }

    Property("isAvailable") {
      return true
    }
  }
}
"#;
        let result = EXPO_MODULES_RESOLVER.extract("ios/HapticsModule.swift", source);
        let node_names = names(&result.nodes);

        assert_contains_all(
            &node_names,
            &[
                "notificationAsync",
                "impactAsync",
                "synchronousThing",
                "isAvailable",
            ],
        );
        assert!(
            result
                .nodes
                .iter()
                .all(|node| node.kind == NodeKind::Method)
        );
        assert!(
            result
                .nodes
                .iter()
                .all(|node| node.qualified_name.contains("ExpoHaptics."))
        );
    }

    #[test]
    fn falls_back_to_the_class_name_when_the_module_has_no_name_x_literal() {
        let source = r#"
public class BareModule: Module {
  public func definition() -> ModuleDefinition {
    Function("doX") { return 1 }
  }
}
"#;
        let result = EXPO_MODULES_RESOLVER.extract("ios/BareModule.swift", source);

        assert!(
            result
                .nodes
                .first()
                .map(|node| node.qualified_name.contains("BareModule.doX"))
                .unwrap_or(false)
        );
    }

    #[test]
    fn returns_no_nodes_for_a_swift_file_that_is_not_an_expo_module() {
        let source = r#"
class Helper {
  func doX() { }
}
"#;
        let result = EXPO_MODULES_RESOLVER.extract("Helper.swift", source);

        assert!(result.nodes.is_empty());
    }

    #[test]
    fn also_extracts_from_kotlin_module_files() {
        let source = r#"
class FooModule : Module() {
    override fun definition() = ModuleDefinition {
        Name("ExpoFoo")
        AsyncFunction("doAsync") { name: String -> name.uppercase() }
        Function("doSync") { 42 }
    }
}
"#;
        let result = EXPO_MODULES_RESOLVER.extract("FooModule.kt", source);

        assert_eq!(result.nodes.len(), 2);
        assert_eq!(sorted(names(&result.nodes)), vec!["doAsync", "doSync"]);
        assert!(
            result
                .nodes
                .iter()
                .all(|node| node.language == Language::Kotlin)
        );
    }
}

mod expo_modules_end_to_end_js_caller_to_native_async_function {
    use super::*;

    #[test]
    fn js_callsite_of_a_literal_async_function_name_resolves_to_the_native_impl_node() {
        let project = TempProject::new();
        project.write(
            "package.json",
            r#"{"dependencies":{"expo-modules-core":"^1.0.0"}}"#,
        );
        project.mkdir("ios");
        project.write(
            "ios/HapticsModule.swift",
            r#"
import ExpoModulesCore
public class HapticsModule: Module {
  public func definition() -> ModuleDefinition {
    Name("ExpoHaptics")
    AsyncFunction("uniqueExpoHapticCall") { in /* ... */ }
  }
}
"#,
        );
        project.mkdir("src");
        project.write(
            "src/index.ts",
            r#"
import { requireNativeModule } from 'expo-modules-core';
const Haptics = requireNativeModule('ExpoHaptics');
export async function impactAsync() {
  return await Haptics.uniqueExpoHapticCall();
}
"#,
        );

        let mut cg = CodeGraph::init_sync(project.path()).expect("CodeGraph should initialize");
        let result = cg.index_all(IndexOptions::default());
        assert!(result.success, "indexing failed: {:?}", result.errors);
        seed_expo_nodes_and_synthesize(
            project.path(),
            &["ios/HapticsModule.swift"],
            Some(("impactAsync", "uniqueExpoHapticCall")),
        );

        let db_path = database_path(project.path());
        let conn = Connection::open(&db_path)
            .unwrap_or_else(|err| panic!("failed to open {}: {err}", db_path.display()));

        // The native method node should exist.
        let native = native_expo_method_ids(&conn, "uniqueExpoHapticCall");
        assert_eq!(native.len(), 1);

        // And the JS callsite should produce a call edge targeting it.
        let call_edge = call_edges_to_expo_method(&conn, "uniqueExpoHapticCall");
        cg.close();
        assert!(!call_edge.is_empty());
        assert_eq!(call_edge[0].target, "uniqueExpoHapticCall");
        assert!(call_edge[0].target_id.starts_with("expo-module:"));
    }

    #[test]
    fn extracts_generic_typed_kotlin_async_function_t_and_pairs_the_ios_android_impls() {
        let project = TempProject::new();
        project.write(
            "package.json",
            r#"{"dependencies":{"expo-modules-core":"^1.0.0"}}"#,
        );
        project.mkdir("ios");
        project.write(
            "ios/BatteryModule.swift",
            r#"import ExpoModulesCore
public class BatteryModule: Module {
  public func definition() -> ModuleDefinition {
    Name("ExpoBattery")
    AsyncFunction("getBatteryLevelAsync") { () -> Float in return 1.0 }
  }
}
"#,
        );
        project.mkdir("android");
        project.write(
            "android/BatteryModule.kt",
            r#"import expo.modules.kotlin.modules.Module
class BatteryModule : Module() {
  override fun definition() = ModuleDefinition {
    Name("ExpoBattery")
    AsyncFunction<Float>("getBatteryLevelAsync") { 1.0f }
  }
}
"#,
        );

        let mut cg = CodeGraph::init_sync(project.path()).expect("CodeGraph should initialize");
        let result = cg.index_all(IndexOptions::default());
        assert!(result.success, "indexing failed: {:?}", result.errors);
        seed_expo_nodes_and_synthesize(
            project.path(),
            &["ios/BatteryModule.swift", "android/BatteryModule.kt"],
            None,
        );

        let db_path = database_path(project.path());
        let conn = Connection::open(&db_path)
            .unwrap_or_else(|err| panic!("failed to open {}: {err}", db_path.display()));

        // The Android (Kotlin) GENERIC AsyncFunction<Float> is extracted --
        // before the fix the `<Float>` defeated the regex and it was silently
        // dropped.
        let kt = kotlin_expo_method_count(&conn, "getBatteryLevelAsync");
        assert_eq!(kt, 1);

        // The iOS (Swift) and Android (Kotlin) impls of the same JS method are
        // linked to each other, so a JS call that resolves to one platform
        // reaches the other.
        let pair = cross_platform_pair_count(&conn, "getBatteryLevelAsync");
        cg.close();
        assert!(pair >= 2);
    }
}
