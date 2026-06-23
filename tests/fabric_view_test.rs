//! React Native Fabric view component extraction and synthesis coverage.
//!
//! This is the Rust port of `__tests__/fabric-view.test.ts`.
//! Direct resolver extraction and end-to-end facade coverage are active.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::Connection;
use rustcodegraph::directory::get_code_graph_dir;
use rustcodegraph::resolution::frameworks::index::FABRIC_VIEW_RESOLVER;
use rustcodegraph::resolution::types::FrameworkResolver;
use rustcodegraph::types::NodeKind;
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
            std::env::temp_dir().join(format!("fabric-fixture-{}-{nanos}", std::process::id()));
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

mod fabric_view_component_extractor_codegen_native_component_specs {
    use super::*;

    #[test]
    fn extracts_a_component_node_and_prop_nodes_from_a_native_ts_spec() {
        let source = r#"
'use client';
import { codegenNativeComponent } from 'react-native';
import type { ViewProps, CodegenTypes as CT, ColorValue } from 'react-native';

type TapEvent = Readonly<{ x: number; y: number }>;

export interface NativeProps extends ViewProps {
  color?: ColorValue;
  onTap?: CT.DirectEventHandler<TapEvent>;
  caption?: string;
}

export default codegenNativeComponent<NativeProps>('MyView', {});
"#;

        let result = FABRIC_VIEW_RESOLVER.extract("src/MyViewNativeComponent.ts", source);
        let component_nodes = result
            .nodes
            .iter()
            .filter(|node| node.kind == NodeKind::Component)
            .collect::<Vec<_>>();
        let prop_nodes = result
            .nodes
            .iter()
            .filter(|node| node.kind == NodeKind::Property)
            .collect::<Vec<_>>();

        assert_eq!(component_nodes.len(), 1);
        assert_eq!(component_nodes[0].name, "MyView");

        let mut prop_names = prop_nodes
            .iter()
            .map(|node| node.name.as_str())
            .collect::<Vec<_>>();
        prop_names.sort();
        assert_eq!(prop_names, vec!["caption", "color", "onTap"]);
    }

    #[test]
    fn returns_nothing_for_a_file_without_codegen_native_component() {
        let source = "export const x = 1;";
        let result = FABRIC_VIEW_RESOLVER.extract("plain.ts", source);

        assert_eq!(result.nodes.len(), 0);
    }

    #[test]
    fn handles_a_spec_with_no_native_props_interface_rare_but_valid() {
        let source = r#"
import { codegenNativeComponent } from 'react-native';
export default codegenNativeComponent('BareComponent');
"#;

        let result = FABRIC_VIEW_RESOLVER.extract("Bare.ts", source);
        // Component node exists; no prop nodes.
        let components = result
            .nodes
            .iter()
            .filter(|node| node.kind == NodeKind::Component)
            .collect::<Vec<_>>();
        let props = result
            .nodes
            .iter()
            .filter(|node| node.kind == NodeKind::Property)
            .collect::<Vec<_>>();

        assert_eq!(components.len(), 1);
        assert_eq!(components[0].name, "BareComponent");
        assert_eq!(props.len(), 0);
    }
}

mod fabric_end_to_end_jsx_consumer_to_fabric_component_to_native_class {
    use super::*;

    #[test]
    fn connects_my_view_jsx_to_the_native_objc_class_via_fabric_synthesizer() {
        let project = TempProject::new();
        project.write(
            "package.json",
            r#"{"dependencies":{"react-native":"^0.73"}}"#,
        );
        // Fabric spec.
        project.mkdir("spec");
        project.write(
            "spec/MyViewNativeComponent.ts",
            "import { codegenNativeComponent } from 'react-native';\n\
             import type { ViewProps } from 'react-native';\n\
             export interface NativeProps extends ViewProps { color?: string; }\n\
             export default codegenNativeComponent<NativeProps>('MyView');",
        );
        // Native iOS implementation - class named with the `View` suffix
        // convention.
        project.mkdir("ios");
        project.write(
            "ios/MyView.mm",
            "@interface MyViewView : UIView\n\
             @end\n\
             @implementation MyViewView\n\
             - (void)setColor:(NSString *)c { /* ... */ }\n\
             @end",
        );
        // JSX consumer.
        project.mkdir("src");
        project.write(
            "src/App.tsx",
            "import React from 'react';\n\
             import MyView from '../spec/MyViewNativeComponent';\n\
             export function App() {\n\
             return <MyView color=\"red\"/>;\n\
             }",
        );

        let mut cg = CodeGraph::init_sync(project.path()).expect("CodeGraph should initialize");
        let result = cg.index_all(IndexOptions::default());
        assert!(result.success, "indexing failed: {:?}", result.errors);

        let db_path = get_code_graph_dir(project.path()).join("rustcodegraph.db");
        let conn = Connection::open(&db_path)
            .unwrap_or_else(|err| panic!("failed to open {}: {err}", db_path.display()));

        // 1. The Fabric component node exists.
        let component_count = conn
            .query_row(
                "SELECT count(*) FROM nodes WHERE id LIKE 'fabric-component:%' AND name='MyView'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .expect("Fabric component query should succeed");
        assert_eq!(component_count, 1);

        // 2. The native class node exists.
        let native_count = conn
            .query_row(
                "SELECT count(*) FROM nodes \
                 WHERE kind='class' AND language='objc' AND name='MyViewView'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .expect("native class query should succeed");
        assert_eq!(native_count, 1);

        // 3. Fabric synthesizer bridges component -> native class.
        let bridge_count = conn
            .query_row(
                "SELECT count(*) FROM edges e \
                 JOIN nodes s ON s.id=e.source \
                 JOIN nodes t ON t.id=e.target \
                 WHERE json_extract(e.metadata,'$.synthesizedBy')='fabric-native-impl' \
                   AND s.name='MyView' AND t.name='MyViewView'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .expect("Fabric bridge query should succeed");
        assert_eq!(bridge_count, 1);

        // 4. JSX synthesizer links the App function -> the Fabric component
        //    (jsx-render edge keyed on the tag name 'MyView').
        let jsx_rows = {
            let mut stmt = conn
                .prepare(
                    "SELECT s.name caller, t.name comp FROM edges e \
                     JOIN nodes s ON s.id=e.source \
                     JOIN nodes t ON t.id=e.target \
                     WHERE json_extract(e.metadata,'$.synthesizedBy')='jsx-render' \
                       AND t.id LIKE 'fabric-component:%' AND t.name='MyView'",
                )
                .expect("JSX edge query should prepare");
            stmt.query_map([], |row| {
                Ok((
                    row.get::<_, String>("caller")?,
                    row.get::<_, String>("comp")?,
                ))
            })
            .expect("JSX edge query should run")
            .collect::<Result<Vec<_>, _>>()
            .expect("JSX edge rows should decode")
        };
        cg.close();

        assert!(!jsx_rows.is_empty());
        assert_eq!(jsx_rows[0].0, "App");
        // The full flow: App (TSX) -> MyView (fabric-component) -> MyViewView
        // (ObjC native class).
    }
}
