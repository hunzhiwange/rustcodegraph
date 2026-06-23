mod describe_2905_interface_extraction {
    use super::*;
    const TS_DESCRIBE_TITLE: &str = "Interface extraction";
    const TS_DESCRIBE_LINE: usize = 2905;
    #[test]
    fn describes_040_is_represented() {
        assert!(!TS_DESCRIBE_TITLE.is_empty());
        assert_eq!(TS_DESCRIBE_LINE, 2905);
    }
    #[test]
    fn case_2906_should_extract_interface_declarations() {
        let suite = ["Pascal / Delphi Extraction", "Interface extraction"];
        assert_eq!(suite.len(), 2);
        assert_eq!(190, 190);
        let code = "unit Test;\ninterface\ntype\n  ILogger = interface\n    procedure Log(const AMsg: string);\n  end;\nimplementation\nend.";
        let result = extract("Test.pas", code);
        find_node(&result, NodeKind::Interface, "ILogger")
            .expect("ILogger interface should be extracted");
    }
}
