//! TS/JS class-field kind classification (#808).
//!
//! Rust port of `__tests__/ts-field-classification.test.ts`.

mod ts_js_class_field_classification_808 {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    use rustcodegraph::types::{EdgeKind, Node, NodeKind, Visibility};
    use rustcodegraph::{CodeGraph, IndexOptions};

    struct TempProject {
        path: PathBuf,
    }

    impl TempProject {
        fn new(prefix: &str) -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time should be after Unix epoch")
                .as_nanos();
            let path =
                std::env::temp_dir().join(format!("{prefix}-{}-{unique}", std::process::id()));
            fs::create_dir_all(&path).expect("temp project directory should be created");
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }

        fn write(&self, name: &str, lines: &[&str]) {
            fs::write(self.path.join(name), lines.join("\n"))
                .expect("fixture source should be written");
        }
    }

    impl Drop for TempProject {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn kind_name(kind: NodeKind) -> &'static str {
        match kind {
            NodeKind::Property => "property",
            NodeKind::Method => "method",
            NodeKind::Function => "function",
            NodeKind::Class => "class",
            _ => "other",
        }
    }

    fn kind_of(cg: &mut CodeGraph, name: &str) -> String {
        let mut kinds = cg
            .get_nodes_by_name(name)
            .into_iter()
            .map(|node| kind_name(node.kind).to_owned())
            .collect::<Vec<_>>();
        kinds.sort();
        kinds.join(",")
    }

    fn node_of_kind(cg: &mut CodeGraph, name: &str, kind: NodeKind) -> Node {
        cg.get_nodes_by_name(name)
            .into_iter()
            .find(|node| node.kind == kind)
            .unwrap_or_else(|| panic!("{name} {kind:?} should be indexed"))
    }

    #[test]
    fn ts_plain_fields_are_properties_function_valued_fields_are_methods() {
        let tmp = TempProject::new("cg-808-ts");
        tmp.write(
            "app.ts",
            &[
                "declare function throttle(f: unknown, ms: number): unknown;",
                "class Fonts {}",
                "class History {}",
                "class App {",
                "  public fonts: Fonts;",
                "  private history: History = new History();",
                "  interactiveCanvas: HTMLCanvasElement | null = null;",
                "  count = 0;",
                "  static defaults = { a: 1 };",
                "  onClick = () => { this.run(); };",
                "  onScroll = throttle((e: Event) => { this.run(); }, 100);",
                "  handler = function namedFn() {};",
                "  handleClick(): void {}",
                "  get value(): number { return 1; }",
                "  run(): void {}",
                "}",
            ],
        );

        let mut cg = CodeGraph::init_sync(tmp.path()).expect("CodeGraph should initialize");
        let result = cg.index_all(IndexOptions::default());
        assert!(result.success, "indexing failed: {:?}", result.errors);

        assert_eq!(kind_of(&mut cg, "fonts"), "property");
        assert_eq!(kind_of(&mut cg, "history"), "property");
        assert_eq!(kind_of(&mut cg, "interactiveCanvas"), "property");
        assert_eq!(kind_of(&mut cg, "count"), "property");
        assert_eq!(kind_of(&mut cg, "defaults"), "property");
        assert_eq!(kind_of(&mut cg, "onClick"), "method");
        assert_eq!(kind_of(&mut cg, "onScroll"), "method");
        assert_eq!(kind_of(&mut cg, "handler"), "method");
        assert_eq!(kind_of(&mut cg, "handleClick"), "method");
        assert_eq!(kind_of(&mut cg, "value"), "method");

        let fonts_prop = node_of_kind(&mut cg, "fonts", NodeKind::Property);
        let fonts_refs = cg
            .get_outgoing_edges(&fonts_prop.id)
            .into_iter()
            .filter(|edge| edge.kind == EdgeKind::References)
            .filter_map(|edge| cg.get_node(&edge.target).map(|node| node.name))
            .collect::<Vec<_>>();
        assert!(fonts_refs.contains(&"Fonts".to_owned()));

        assert_eq!(fonts_prop.visibility, Some(Visibility::Public));
        let history_prop = node_of_kind(&mut cg, "history", NodeKind::Property);
        assert_eq!(history_prop.visibility, Some(Visibility::Private));

        let on_click = cg
            .get_nodes_by_name("onClick")
            .into_iter()
            .next()
            .expect("onClick should be indexed");
        let calls = cg
            .get_outgoing_edges(&on_click.id)
            .into_iter()
            .filter(|edge| edge.kind == EdgeKind::Calls)
            .filter_map(|edge| cg.get_node(&edge.target).map(|node| node.name))
            .collect::<Vec<_>>();
        assert!(calls.contains(&"run".to_owned()));

        assert_eq!(fonts_prop.signature.as_deref(), Some("Fonts fonts"));

        cg.destroy();
    }

    #[test]
    fn js_field_definition_classifies_the_same_way() {
        let tmp = TempProject::new("cg-808-js");
        tmp.write(
            "app.js",
            &[
                "class App {",
                "  count = 0;",
                "  config = { retries: 3 };",
                "  onClick = () => { this.run(); };",
                "  run() {}",
                "}",
                "module.exports = App;",
            ],
        );

        let mut cg = CodeGraph::init_sync(tmp.path()).expect("CodeGraph should initialize");
        let result = cg.index_all(IndexOptions::default());
        assert!(result.success, "indexing failed: {:?}", result.errors);

        assert_eq!(
            cg.get_nodes_by_name("count").first().map(|node| node.kind),
            Some(NodeKind::Property)
        );
        assert_eq!(
            cg.get_nodes_by_name("config").first().map(|node| node.kind),
            Some(NodeKind::Property)
        );
        assert_eq!(
            cg.get_nodes_by_name("onClick")
                .first()
                .map(|node| node.kind),
            Some(NodeKind::Method)
        );

        cg.destroy();
    }

    #[test]
    fn field_initializers_still_register_callbacks_fn_ref_scan() {
        let tmp = TempProject::new("cg-808-fnref");
        tmp.write(
            "main.ts",
            &[
                "function onSave(): void {}",
                "function onLoad(): void {}",
                "export class Registry {",
                "  static handlers = { save: onSave, load: onLoad };",
                "}",
            ],
        );

        let mut cg = CodeGraph::init_sync(tmp.path()).expect("CodeGraph should initialize");
        let result = cg.index_all(IndexOptions::default());
        assert!(result.success, "indexing failed: {:?}", result.errors);

        let on_save = cg
            .get_nodes_by_name("onSave")
            .into_iter()
            .next()
            .expect("onSave should be indexed");
        let fn_refs = cg
            .get_incoming_edges(&on_save.id)
            .into_iter()
            .filter(|edge| {
                edge.metadata
                    .as_ref()
                    .and_then(|metadata| metadata.get("fnRef"))
                    .and_then(|value| value.as_bool())
                    == Some(true)
            })
            .count();
        assert!(fn_refs > 0);

        cg.destroy();
    }
}
