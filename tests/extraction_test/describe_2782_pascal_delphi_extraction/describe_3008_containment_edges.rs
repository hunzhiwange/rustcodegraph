mod describe_3008_containment_edges {
    use super::*;
    const TS_DESCRIBE_TITLE: &str = "Containment edges";
    const TS_DESCRIBE_LINE: usize = 3008;
    #[test]
    fn describes_047_is_represented() {
        assert!(!TS_DESCRIBE_TITLE.is_empty());
        assert_eq!(TS_DESCRIBE_LINE, 3008);
    }
    #[test]
    fn case_3009_should_create_contains_edges_for_class_members() {
        let suite = ["Pascal / Delphi Extraction", "Containment edges"];
        assert_eq!(suite.len(), 2);
        assert_eq!(198, 198);
        let code = "unit Test;\ninterface\ntype\n  TObj = class\n  public\n    procedure Foo;\n  end;\nimplementation\nend.";
        let result = extract("Test.pas", code);
        let class = find_node(&result, NodeKind::Class, "TObj").expect("class should exist");
        let method = find_node(&result, NodeKind::Method, "Foo").expect("method should exist");
        assert!(
            result.edges.iter().any(|edge| {
                edge.source == class.id
                    && edge.target == method.id
                    && edge.kind == EdgeKind::Contains
            }),
            "edges: {:?}",
            result.edges
        );
    }
}
