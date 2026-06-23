mod native_indexing_runtime {
    use super::*;

    #[test]
    fn index_all_uses_native_parser_output_for_core_languages() {
        let test_dir = create_temp_dir();
        write_fixture(
            test_dir.path().join("math.ts"),
            "export function add(a: number, b: number): number { return a + b; }\n",
        );
        write_fixture(
            test_dir.path().join("math.js"),
            "export function sub(a, b) { return a - b; }\n",
        );
        write_fixture(
            test_dir.path().join("lib.rs"),
            "pub fn rust_add(a: i32, b: i32) -> i32 { a + b }\n",
        );
        write_fixture(
            test_dir.path().join("main.go"),
            "package main\nfunc goAdd(a int, b int) int { return a + b }\n",
        );
        write_fixture(
            test_dir.path().join("main.py"),
            "def py_add(a, b):\n    return a + b\n",
        );
        write_fixture(
            test_dir.path().join("Main.java"),
            "class Main { int javaAdd(int a, int b) { return a + b; } }\n",
        );

        let mut cg = CodeGraph::init(test_dir.path(), Default::default())
            .expect("CodeGraph should initialize");
        let result = cg.index_all(IndexOptions::default());
        assert!(result.success, "indexing failed: {:?}", result.errors);

        for (name, kind, language) in [
            ("add", NodeKind::Function, Language::TypeScript),
            ("sub", NodeKind::Function, Language::JavaScript),
            ("rust_add", NodeKind::Function, Language::Rust),
            ("goAdd", NodeKind::Function, Language::Go),
            ("py_add", NodeKind::Function, Language::Python),
            ("javaAdd", NodeKind::Method, Language::Java),
        ] {
            let nodes = cg.get_nodes_by_name(name);
            assert!(
                nodes
                    .iter()
                    .any(|node| node.kind == kind && node.language == language),
                "expected {language:?} {kind:?} node named {name}, got {nodes:?}"
            );
        }
    }
}
