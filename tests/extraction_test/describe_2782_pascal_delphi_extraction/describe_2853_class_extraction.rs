mod describe_2853_class_extraction {
    use super::*;
    const TS_DESCRIBE_TITLE: &str = "Class extraction";
    const TS_DESCRIBE_LINE: usize = 2853;
    #[test]
    fn describes_038_is_represented() {
        assert!(!TS_DESCRIBE_TITLE.is_empty());
        assert_eq!(TS_DESCRIBE_LINE, 2853);
    }
    #[test]
    fn case_2854_should_extract_class_declarations() {
        let suite = ["Pascal / Delphi Extraction", "Class extraction"];
        assert_eq!(suite.len(), 2);
        assert_eq!(186, 186);
        let code = "unit Test;\ninterface\ntype\n  TMyClass = class\n  public\n    procedure DoSomething;\n  end;\nimplementation\nend.";
        let result = extract("Test.pas", code);
        find_node(&result, NodeKind::Class, "TMyClass")
            .expect("TMyClass class should be extracted");
    }
    #[test]
    fn case_2863_should_extract_class_with_inheritance() {
        let suite = ["Pascal / Delphi Extraction", "Class extraction"];
        assert_eq!(suite.len(), 2);
        assert_eq!(187, 187);
        let code =
            "unit Test;\ninterface\ntype\n  TChild = class(TParent)\n  end;\nimplementation\nend.";
        let result = extract("Test.pas", code);
        assert_contains(
            &references_by_kind(&result, ReferenceKind::Extends),
            "TParent",
        );
    }
    #[test]
    fn case_2874_should_extract_class_with_interface_implementation() {
        let suite = ["Pascal / Delphi Extraction", "Class extraction"];
        assert_eq!(suite.len(), 2);
        assert_eq!(188, 188);
        let code = "unit Test;\ninterface\ntype\n  TService = class(TInterfacedObject, ILogger)\n  end;\nimplementation\nend.";
        let result = extract("Test.pas", code);
        assert_contains(
            &references_by_kind(&result, ReferenceKind::Extends),
            "TInterfacedObject",
        );
        assert_contains(
            &references_by_kind(&result, ReferenceKind::Implements),
            "ILogger",
        );
    }
}
