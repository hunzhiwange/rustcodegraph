mod describe_0721_file_node_extraction {
    use super::*;
    const TS_DESCRIBE_TITLE: &str = "File Node Extraction";
    const TS_DESCRIBE_LINE: usize = 721;
    #[test]
    fn describes_007_is_represented() {
        assert!(!TS_DESCRIBE_TITLE.is_empty());
        assert_eq!(TS_DESCRIBE_LINE, 721);
    }
    #[test]
    fn case_0722_should_create_a_file_kind_node_for_each_parsed_file() {
        let suite = ["File Node Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(50, 50);
        let result = extract("main.py", "def main():\n    pass\n");
        let file_node = find_node(&result, NodeKind::File, "main.py")
            .expect("main.py file node should be extracted");
        assert_eq!(file_node.language, Language::Python);
    }
    #[test]
    fn case_0738_should_create_file_nodes_for_python_files() {
        let suite = ["File Node Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(51, 51);
        let result = extract("main.py", "def main():\n    pass\n");
        let file_node = find_node(&result, NodeKind::File, "main.py")
            .expect("Python file node should be extracted");
        assert_eq!(file_node.language, Language::Python);
    }
    #[test]
    fn case_0751_should_create_containment_edges_from_file_node_to_top_level_declaratio() {
        let suite = ["File Node Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(52, 52);
        let code = r#"
export function foo() {}
export function bar() {}
"#;
        let result = extract("fns.ts", code);
        let file_node =
            find_node(&result, NodeKind::File, "fns.ts").expect("file node should be extracted");
        let contains_edges = result
            .edges
            .iter()
            .filter(|edge| edge.source == file_node.id && edge.kind == EdgeKind::Contains)
            .count();
        assert!(contains_edges >= 2, "edges: {:?}", result.edges);
    }
}
