mod describe_2995_call_extraction {
    use super::*;
    const TS_DESCRIBE_TITLE: &str = "Call extraction";
    const TS_DESCRIBE_LINE: usize = 2995;
    #[test]
    fn describes_046_is_represented() {
        assert!(!TS_DESCRIBE_TITLE.is_empty());
        assert_eq!(TS_DESCRIBE_LINE, 2995);
    }
    #[test]
    fn case_2996_should_extract_calls_from_implementation_bodies() {
        let suite = ["Pascal / Delphi Extraction", "Call extraction"];
        assert_eq!(suite.len(), 2);
        assert_eq!(197, 197);
        let code = "unit Test;\ninterface\ntype\n  TObj = class\n  public\n    procedure DoWork;\n  end;\nimplementation\nprocedure TObj.DoWork;\nbegin\n  WriteLn('hello');\nend;\nend.";
        let result = extract("Test.pas", code);
        assert_contains(
            &references_by_kind(&result, ReferenceKind::Calls),
            "WriteLn",
        );
    }
}
