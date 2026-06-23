mod describe_2916_method_extraction {
    use super::*;
    const TS_DESCRIBE_TITLE: &str = "Method extraction";
    const TS_DESCRIBE_LINE: usize = 2916;
    #[test]
    fn describes_041_is_represented() {
        assert!(!TS_DESCRIBE_TITLE.is_empty());
        assert_eq!(TS_DESCRIBE_LINE, 2916);
    }
    #[test]
    fn case_2917_should_extract_methods_with_visibility() {
        let suite = ["Pascal / Delphi Extraction", "Method extraction"];
        assert_eq!(suite.len(), 2);
        assert_eq!(191, 191);
        let code = "unit Test;\ninterface\ntype\n  TMyClass = class\n  private\n    FValue: Integer;\n  public\n    constructor Create;\n    function GetValue: Integer;\n  end;\nimplementation\nend.";
        let result = extract("Test.pas", code);
        let methods = nodes_by_kind(&result, NodeKind::Method);
        assert_eq!(methods.len(), 2, "methods: {methods:?}");
        assert_eq!(
            find_node(&result, NodeKind::Method, "Create")
                .and_then(|node| node.visibility)
                .as_ref(),
            Some(&Visibility::Public)
        );
        assert_eq!(
            find_node(&result, NodeKind::Method, "GetValue")
                .and_then(|node| node.visibility)
                .as_ref(),
            Some(&Visibility::Public)
        );
        assert_eq!(
            find_node(&result, NodeKind::Field, "FValue")
                .and_then(|node| node.visibility)
                .as_ref(),
            Some(&Visibility::Private)
        );
    }
    #[test]
    fn case_2935_should_detect_static_methods_class_methods() {
        let suite = ["Pascal / Delphi Extraction", "Method extraction"];
        assert_eq!(suite.len(), 2);
        assert_eq!(192, 192);
        let code = "unit Test;\ninterface\ntype\n  THelper = class\n  public\n    class function Create: THelper; static;\n  end;\nimplementation\nend.";
        let result = extract("Test.pas", code);
        let method = find_node(&result, NodeKind::Method, "Create")
            .expect("static class method should be extracted");
        assert_eq!(method.is_static, Some(true));
    }
}
