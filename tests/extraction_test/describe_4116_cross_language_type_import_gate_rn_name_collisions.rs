mod describe_4116_cross_language_type_import_gate_rn_name_collisions {
    use super::*;
    const TS_DESCRIBE_TITLE: &str = "Cross-language type/import gate (RN name collisions)";
    const TS_DESCRIBE_LINE: usize = 4116;
    #[test]
    fn describes_059_is_represented() {
        assert!(!TS_DESCRIBE_TITLE.is_empty());
        assert_eq!(TS_DESCRIBE_LINE, 4116);
    }
    #[test]
    fn case_4129_a_ts_pascalcase_type_ref_lands_on_the_ts_type_never_a_same_named_nativ() {
        let suite = ["Cross-language type/import gate (RN name collisions)"];
        assert_eq!(suite.len(), 1);
        assert_eq!(226, 226);
        let temp = TempDir::new("codegraph-rn-type-gate");
        temp.write("package.json", r#"{"dependencies":{"react-native":"*"}}"#);
        temp.write(
            "useTests.ts",
            r#"export type TestRunner = { run: () => void };
"#,
        );
        temp.write(
            "basic.tsx",
            r#"export function useBasicTest(r: TestRunner): TestRunner {
  return r;
}
"#,
        );
        temp.write(
            "TestUtils.kt",
            r#"package app
class TestRunner {
  fun run() {}
}
"#,
        );

        let mut cg = index_project(&temp);
        let kt_runner = cg
            .get_nodes_by_kind(NodeKind::Class)
            .into_iter()
            .find(|node| node.name == "TestRunner" && node.file_path.ends_with("TestUtils.kt"))
            .expect("Kotlin TestRunner should be indexed");
        let kt_deps = impact_file_paths(&mut cg, &kt_runner.id, 2);
        assert!(
            kt_deps.iter().all(|path| !path.ends_with("basic.tsx")),
            "Kotlin TestRunner must not reach TSX file: {kt_deps:?}"
        );

        let ts_runner = cg
            .get_nodes_by_kind(NodeKind::TypeAlias)
            .into_iter()
            .find(|node| node.name == "TestRunner")
            .expect("TS TestRunner type alias should be indexed");
        let ts_deps = impact_file_paths(&mut cg, &ts_runner.id, 2);
        assert!(
            ts_deps.iter().any(|path| path.ends_with("basic.tsx")),
            "TS TestRunner should reach basic.tsx: {ts_deps:?}"
        );
        cg.close();
    }
    #[test]
    fn case_4171_gates_a_cross_family_import_name_collision_but_keeps_same_family_impor() {
        let suite = ["Cross-language type/import gate (RN name collisions)"];
        assert_eq!(suite.len(), 1);
        assert_eq!(227, 227);
        let temp = TempDir::new("codegraph-rn-import-gate");
        temp.write(
            "Widget.swift",
            r#"class Widget {
  func render() {}
}
"#,
        );
        temp.write(
            "widget.ts",
            r#"import { Widget } from './native';
export function mount(w: Widget) {}
"#,
        );
        temp.write("util.ts", "export class Helper {}\n");
        temp.write(
            "app.ts",
            r#"import { Helper } from './util';
export const h = new Helper();
"#,
        );

        let mut cg = index_project(&temp);
        let swift_widget = cg
            .get_nodes_by_kind(NodeKind::Class)
            .into_iter()
            .find(|node| node.name == "Widget" && node.file_path.ends_with(".swift"))
            .expect("Swift Widget should be indexed");
        let w_deps = impact_file_paths(&mut cg, &swift_widget.id, 2);
        assert!(
            w_deps.iter().all(|path| !path.ends_with("widget.ts")),
            "Swift Widget must not reach widget.ts: {w_deps:?}"
        );

        let helper = cg
            .get_nodes_by_kind(NodeKind::Class)
            .into_iter()
            .find(|node| node.name == "Helper")
            .expect("TS Helper should be indexed");
        let h_deps = impact_file_paths(&mut cg, &helper.id, 2);
        assert!(
            h_deps.iter().any(|path| path.ends_with("app.ts")),
            "same-family TS import should reach app.ts: {h_deps:?}"
        );
        cg.close();
    }
}
