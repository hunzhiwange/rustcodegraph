//! React Native bridge resolver.
//!
//! Rust port of `__tests__/react-native-bridge.test.ts`.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::Connection;
use rustcodegraph::directory::get_code_graph_dir;
use rustcodegraph::resolution::frameworks::index::REACT_NATIVE_BRIDGE_RESOLVER;
use rustcodegraph::resolution::types::{
    FrameworkResolver, ImportMapping, ResolutionContext, ResolvedBy, UnresolvedRef, language_name,
    now_ms,
};
use rustcodegraph::types::{Language, Node, NodeKind, ReferenceKind};
use rustcodegraph::{CodeGraph, IndexOptions};

/// Mock ResolutionContext for the React Native bridge resolver.
struct MockResolutionContext {
    nodes: Vec<Node>,
    file_contents: Vec<(String, String)>,
}

fn make_context(
    nodes: Vec<Node>,
    file_contents: impl IntoIterator<Item = (&'static str, &'static str)>,
) -> MockResolutionContext {
    MockResolutionContext {
        nodes,
        file_contents: file_contents
            .into_iter()
            .map(|(path, content)| (path.to_owned(), content.to_owned()))
            .collect(),
    }
}

impl MockResolutionContext {
    fn all_files(&self) -> Vec<String> {
        // Files = union of node files + any extra fileContents keys (for files
        // that have content like .mm bridge declarations but no extracted nodes yet).
        let mut seen = HashSet::new();
        let mut files = Vec::new();
        for node in &self.nodes {
            if seen.insert(node.file_path.clone()) {
                files.push(node.file_path.clone());
            }
        }
        for (file_path, _) in &self.file_contents {
            if seen.insert(file_path.clone()) {
                files.push(file_path.clone());
            }
        }
        files
    }
}

impl ResolutionContext for MockResolutionContext {
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

    fn get_nodes_by_qualified_name(&mut self, _qualified_name: &str) -> Vec<Node> {
        panic!("not used")
    }

    fn get_nodes_by_kind(&mut self, kind: NodeKind) -> Vec<Node> {
        self.nodes
            .iter()
            .filter(|node| node.kind == kind)
            .cloned()
            .collect()
    }

    fn file_exists(&mut self, file_path: &str) -> bool {
        self.all_files().iter().any(|path| path == file_path)
    }

    fn read_file(&mut self, file_path: &str) -> Option<String> {
        self.file_contents
            .iter()
            .find_map(|(path, content)| (path == file_path).then(|| content.clone()))
    }

    fn get_project_root(&self) -> String {
        "/test".to_owned()
    }

    fn get_all_files(&mut self) -> Vec<String> {
        self.all_files()
    }

    fn get_nodes_by_lower_name(&mut self, _lower_name: &str) -> Vec<Node> {
        panic!("not used")
    }

    fn get_import_mappings(&mut self, _file_path: &str, _language: Language) -> Vec<ImportMapping> {
        Vec::new()
    }
}

fn method(name: &str, language: Language, file_path: &str, start_line: u64) -> Node {
    Node {
        id: format!(
            "{}:{file_path}:{name}:{start_line}",
            language_name(language)
        ),
        kind: NodeKind::Method,
        name: name.to_owned(),
        qualified_name: format!("{file_path}::{name}"),
        file_path: file_path.to_owned(),
        language,
        start_line,
        end_line: start_line + 5,
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

fn reference(name: &str, language: Language, file_path: &str) -> UnresolvedRef {
    UnresolvedRef {
        from_node_id: format!("caller:{file_path}"),
        reference_name: name.to_owned(),
        reference_kind: ReferenceKind::Calls,
        line: 1,
        column: 0,
        file_path: file_path.to_owned(),
        language,
        candidates: None,
    }
}

fn temp_root(prefix: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after the Unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("{prefix}-{}-{unique}", std::process::id()))
}

struct TempProject {
    root: PathBuf,
}

impl TempProject {
    fn new(prefix: &str) -> Self {
        let root = temp_root(prefix);
        fs::create_dir_all(&root)
            .unwrap_or_else(|err| panic!("failed to create temp dir {}: {err}", root.display()));
        Self { root }
    }

    fn path(&self) -> &Path {
        &self.root
    }

    fn write(&self, relative_path: &str, content: &str) {
        fs::write(self.root.join(relative_path), content)
            .unwrap_or_else(|err| panic!("failed to write {relative_path}: {err}"));
    }
}

impl Drop for TempProject {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

mod react_native_bridge_resolver {
    use super::*;

    mod detect {
        use super::*;

        #[test]
        fn returns_true_when_package_json_declares_react_native() {
            let mut ctx = make_context(
                Vec::new(),
                [(
                    "package.json",
                    r#"{"name":"x","dependencies":{"react-native":"^0.73.0"}}"#,
                )],
            );

            assert!(REACT_NATIVE_BRIDGE_RESOLVER.detect(&mut ctx));
        }

        #[test]
        fn returns_true_when_an_objc_file_uses_rct_export_module() {
            let mut ctx = make_context(
                Vec::new(),
                [(
                    "NativeFoo.mm",
                    "@implementation Foo\nRCT_EXPORT_MODULE()\n@end",
                )],
            );

            assert!(REACT_NATIVE_BRIDGE_RESOLVER.detect(&mut ctx));
        }

        #[test]
        fn returns_true_when_a_ts_file_uses_turbo_module_registry() {
            let mut ctx = make_context(
                Vec::new(),
                [(
                    "NativeFoo.ts",
                    "import { TurboModuleRegistry } from 'react-native';\n\
                     export default TurboModuleRegistry.getEnforcing<Spec>('Foo');",
                )],
            );

            assert!(REACT_NATIVE_BRIDGE_RESOLVER.detect(&mut ctx));
        }

        #[test]
        fn returns_false_when_none_of_the_rn_signals_are_present() {
            let mut ctx = make_context(
                vec![method("hi", Language::ObjC, "X.m", 10)],
                Vec::<(&'static str, &'static str)>::new(),
            );

            assert!(!REACT_NATIVE_BRIDGE_RESOLVER.detect(&mut ctx));
        }
    }

    mod legacy_bridge_objc_side {
        use super::*;

        #[test]
        fn resolves_js_callsite_via_rct_export_method_with_default_module_name() {
            // RCTGeolocation -> module name "Geolocation" (RCT prefix stripped).
            let native = method(
                "getCurrentPosition:",
                Language::ObjC,
                "RCTGeolocation.m",
                10,
            );
            let mut ctx = make_context(
                vec![native.clone()],
                [
                    (
                        "package.json",
                        r#"{"dependencies":{"react-native":"^0.73"}}"#,
                    ),
                    (
                        "RCTGeolocation.m",
                        "@implementation RCTGeolocation\n\
                         RCT_EXPORT_MODULE()\n\
                         RCT_EXPORT_METHOD(getCurrentPosition:(RCTResponseSenderBlock)cb) {}\n\
                         @end",
                    ),
                ],
            );

            let result = REACT_NATIVE_BRIDGE_RESOLVER.resolve(
                &reference("getCurrentPosition", Language::JavaScript, "App.js"),
                &mut ctx,
            );

            let result = result.expect("getCurrentPosition should resolve");
            assert_eq!(result.target_node_id, native.id);
            assert_eq!(result.resolved_by, ResolvedBy::Framework);
        }

        #[test]
        fn resolves_via_explicit_module_name_in_rct_export_module_name() {
            let native = method("startScan:", Language::ObjC, "Bluetooth.m", 10);
            let mut ctx = make_context(
                vec![native.clone()],
                [
                    (
                        "package.json",
                        r#"{"dependencies":{"react-native":"^0.73"}}"#,
                    ),
                    (
                        "Bluetooth.m",
                        "@implementation BluetoothImpl\n\
                         RCT_EXPORT_MODULE(BluetoothManager)\n\
                         RCT_EXPORT_METHOD(startScan:(RCTResponseSenderBlock)cb) {}\n\
                         @end",
                    ),
                ],
            );

            let result = REACT_NATIVE_BRIDGE_RESOLVER.resolve(
                &reference("startScan", Language::JavaScript, "App.js"),
                &mut ctx,
            );

            assert_eq!(
                result.map(|resolved| resolved.target_node_id),
                Some(native.id)
            );
        }

        #[test]
        fn resolves_rct_remap_method_with_js_name_override() {
            let native = method("doInternalCompute:", Language::ObjC, "Computer.m", 10);
            let mut ctx = make_context(
                vec![native.clone()],
                [
                    (
                        "package.json",
                        r#"{"dependencies":{"react-native":"^0.73"}}"#,
                    ),
                    (
                        "Computer.m",
                        "@implementation Computer\n\
                         RCT_EXPORT_MODULE()\n\
                         RCT_REMAP_METHOD(compute, doInternalCompute:(NSDictionary *)opts) {}\n\
                         @end",
                    ),
                ],
            );

            let result = REACT_NATIVE_BRIDGE_RESOLVER.resolve(
                &reference("compute", Language::JavaScript, "App.js"),
                &mut ctx,
            );

            assert_eq!(
                result.map(|resolved| resolved.target_node_id),
                Some(native.id)
            );
        }
    }

    mod legacy_bridge_java_side {
        use super::*;

        #[test]
        fn resolves_react_method_with_get_name_literal() {
            let native = method(
                "getCurrentPosition",
                Language::Java,
                "GeolocationModule.java",
                10,
            );
            let mut ctx = make_context(
                vec![native.clone()],
                [
                    (
                        "package.json",
                        r#"{"dependencies":{"react-native":"^0.73"}}"#,
                    ),
                    (
                        "GeolocationModule.java",
                        "class GeolocationModule extends ReactContextBaseJavaModule {\n\
                         @Override public String getName() { return \"Geolocation\"; }\n\
                         @ReactMethod public void getCurrentPosition(Callback cb) {}\n\
                         }",
                    ),
                ],
            );

            let result = REACT_NATIVE_BRIDGE_RESOLVER.resolve(
                &reference("getCurrentPosition", Language::JavaScript, "App.js"),
                &mut ctx,
            );

            assert_eq!(
                result.map(|resolved| resolved.target_node_id),
                Some(native.id)
            );
        }

        #[test]
        fn resolves_kotlin_react_method_fun() {
            let native = method("startScan", Language::Kotlin, "BluetoothModule.kt", 10);
            let mut ctx = make_context(
                vec![native.clone()],
                [
                    (
                        "package.json",
                        r#"{"dependencies":{"react-native":"^0.73"}}"#,
                    ),
                    (
                        "BluetoothModule.kt",
                        "class BluetoothModule(ctx: ReactApplicationContext) : ReactContextBaseJavaModule(ctx) {\n\
                         override fun getName(): String = \"BluetoothManager\"\n\
                         @ReactMethod fun startScan(cb: Callback) {}\n\
                         }",
                    ),
                ],
            );

            let result = REACT_NATIVE_BRIDGE_RESOLVER.resolve(
                &reference("startScan", Language::JavaScript, "App.js"),
                &mut ctx,
            );

            assert_eq!(
                result.map(|resolved| resolved.target_node_id),
                Some(native.id)
            );
        }
    }

    mod turbo_module_spec_resolution {
        use super::*;

        #[test]
        fn matches_spec_method_to_native_objc_implementation_by_name() {
            // The Spec interface lists `getTotalLength`; ObjC has a method by
            // the same first keyword. Bridge matches by name.
            let native = method(
                "getTotalLength:",
                Language::ObjC,
                "RNSVGRenderableManager.mm",
                10,
            );
            let mut ctx = make_context(
                vec![native.clone()],
                [
                    (
                        "package.json",
                        r#"{"dependencies":{"react-native":"^0.73"}}"#,
                    ),
                    (
                        "NativeSvgRenderableModule.ts",
                        "import { TurboModuleRegistry } from 'react-native';\n\
                         export interface Spec extends TurboModule {\n\
                         getTotalLength(tag: number): number;\n\
                         isPointInFill(tag: number, options?: object): boolean;\n\
                         }\n\
                         export default TurboModuleRegistry.getEnforcing<Spec>('RNSVGRenderableModule');",
                    ),
                ],
            );

            let result = REACT_NATIVE_BRIDGE_RESOLVER.resolve(
                &reference("getTotalLength", Language::Tsx, "SvgComponent.tsx"),
                &mut ctx,
            );

            assert_eq!(
                result.map(|resolved| resolved.target_node_id),
                Some(native.id)
            );
        }

        #[test]
        fn returns_null_when_spec_method_has_no_matching_native_impl() {
            let mut ctx = make_context(
                Vec::new(),
                [
                    (
                        "package.json",
                        r#"{"dependencies":{"react-native":"^0.73"}}"#,
                    ),
                    (
                        "NativeFoo.ts",
                        "import { TurboModuleRegistry } from 'react-native';\n\
                         export interface Spec extends TurboModule {\n\
                         thingThatDoesntExist(): void;\n\
                         }\n\
                         export default TurboModuleRegistry.getEnforcing<Spec>('Foo');",
                    ),
                ],
            );

            let result = REACT_NATIVE_BRIDGE_RESOLVER.resolve(
                &reference("thingThatDoesntExist", Language::Tsx, "Caller.tsx"),
                &mut ctx,
            );

            assert!(result.is_none());
        }
    }

    mod qualified_vs_bare_callsite_names {
        use super::*;

        #[test]
        fn handles_bare_method_name_post_receiver_strip() {
            let native = method("compute:", Language::ObjC, "Mod.m", 10);
            let mut ctx = make_context(
                vec![native],
                [
                    (
                        "package.json",
                        r#"{"dependencies":{"react-native":"^0.73"}}"#,
                    ),
                    (
                        "Mod.m",
                        "@implementation Mod\n\
                         RCT_EXPORT_MODULE()\n\
                         RCT_EXPORT_METHOD(compute:(NSDictionary *)x) {}\n\
                         @end",
                    ),
                ],
            );

            assert!(
                REACT_NATIVE_BRIDGE_RESOLVER
                    .resolve(
                        &reference("compute", Language::JavaScript, "App.js"),
                        &mut ctx
                    )
                    .is_some()
            );
        }

        #[test]
        fn strips_dot_prefix_on_receiver_qualified_callsite_native_modules_mod_compute_to_compute()
        {
            let native = method("compute:", Language::ObjC, "Mod.m", 10);
            let mut ctx = make_context(
                vec![native],
                [
                    (
                        "package.json",
                        r#"{"dependencies":{"react-native":"^0.73"}}"#,
                    ),
                    (
                        "Mod.m",
                        "@implementation Mod\n\
                         RCT_EXPORT_MODULE()\n\
                         RCT_EXPORT_METHOD(compute:(NSDictionary *)x) {}\n\
                         @end",
                    ),
                ],
            );

            assert!(
                REACT_NATIVE_BRIDGE_RESOLVER
                    .resolve(
                        &reference("NativeModules.Mod.compute", Language::JavaScript, "App.js"),
                        &mut ctx
                    )
                    .is_some()
            );
        }
    }

    #[test]
    fn does_not_resolve_native_language_callers_resolver_is_js_side_only() {
        let native = method("compute:", Language::ObjC, "Mod.m", 10);
        let mut ctx = make_context(vec![native], Vec::<(&'static str, &'static str)>::new());

        let result = REACT_NATIVE_BRIDGE_RESOLVER.resolve(
            &reference("compute", Language::ObjC, "OtherMod.m"),
            &mut ctx,
        );

        assert!(result.is_none());
    }

    mod rct_event_emitter_built_ins_blocklist {
        use super::*;

        #[test]
        fn skips_add_listener_remove_every_emitter_exposes_these_bridging_them_creates_noise() {
            // A repo with RCTEventEmitter subclass defines `addListener:` and
            // `remove:` because that is what `[RCTEventEmitter addListener:]`
            // requires. JS callers of `.addListener(...)` should not resolve
            // here; they hit the JS-side `NativeEventEmitter` abstraction.
            let native1 = method("addListener:", Language::ObjC, "EventEmitter.m", 10);
            let native2 = method("remove:", Language::ObjC, "EventEmitter.m", 10);
            let mut ctx = make_context(
                vec![native1, native2],
                [
                    (
                        "package.json",
                        r#"{"dependencies":{"react-native":"^0.73"}}"#,
                    ),
                    (
                        "EventEmitter.m",
                        "@implementation EventEmitter\n\
                         RCT_EXPORT_MODULE()\n\
                         RCT_EXPORT_METHOD(addListener:(NSString *)eventName) {}\n\
                         RCT_EXPORT_METHOD(remove:(double)id) {}\n\
                         @end",
                    ),
                ],
            );

            assert!(
                REACT_NATIVE_BRIDGE_RESOLVER
                    .resolve(
                        &reference("addListener", Language::JavaScript, "App.js"),
                        &mut ctx
                    )
                    .is_none()
            );
            assert!(
                REACT_NATIVE_BRIDGE_RESOLVER
                    .resolve(
                        &reference("remove", Language::TypeScript, "App.ts"),
                        &mut ctx
                    )
                    .is_none()
            );
        }
    }
}

mod react_native_cross_platform_pairing_end_to_end {
    use super::*;

    #[test]
    fn links_the_android_react_method_and_ios_rct_export_method_impls_of_a_js_called_method() {
        let project = TempProject::new("rn-xplat");
        project.write(
            "package.json",
            r#"{"dependencies":{"react-native":"^0.74.0"}}"#,
        );
        project.write(
            "index.ts",
            "import { NativeModules } from 'react-native';\n\
             export function ping() { return NativeModules.RNThing.uniquePingMethod(); }\n",
        );
        project.write(
            "RNThing.java",
            "public class RNThing extends ReactContextBaseJavaModule {\n\
             @Override public String getName() { return \"RNThing\"; }\n\
             @ReactMethod public void uniquePingMethod(Callback cb) {}\n\
             }\n",
        );
        project.write(
            "RNThing.m",
            "@implementation RNThing\n\
             RCT_EXPORT_MODULE()\n\
             RCT_EXPORT_METHOD(uniquePingMethod:(RCTResponseSenderBlock)cb) {}\n\
             @end\n",
        );

        let mut cg = CodeGraph::init_sync(project.path()).expect("CodeGraph should initialize");
        let result = cg.index_all(IndexOptions::default());
        assert!(result.success, "indexing failed: {:?}", result.errors);

        let db_path = get_code_graph_dir(project.path()).join("rustcodegraph.db");
        let conn = Connection::open(&db_path)
            .unwrap_or_else(|err| panic!("failed to open {}: {err}", db_path.display()));

        // The iOS `RCT_EXPORT_METHOD` is extracted as an ObjC method node (the
        // macro parses as a macro-expression, not a method, so it had no node before).
        let objc_count = conn
            .query_row(
                "SELECT count(*) FROM nodes \
                 WHERE name='uniquePingMethod' \
                   AND language='objc' \
                   AND id LIKE 'rn-export:%'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .expect("ObjC export query should succeed");
        assert_eq!(objc_count, 1);

        // The Java and ObjC impls of `uniquePingMethod` are linked to each other,
        // so a JS call that resolves to one platform reaches the other.
        let pair_count = conn
            .query_row(
                "SELECT count(*) FROM edges e \
                 JOIN nodes s ON s.id=e.source \
                 JOIN nodes t ON t.id=e.target \
                 WHERE json_extract(e.metadata,'$.synthesizedBy')='rn-cross-platform' \
                   AND s.name LIKE 'uniquePingMethod%' \
                   AND t.name LIKE 'uniquePingMethod%' \
                   AND s.language != t.language",
                [],
                |row| row.get::<_, i64>(0),
            )
            .expect("cross-platform edge query should succeed");
        cg.close();
        assert!(pair_count >= 2);
    }
}
