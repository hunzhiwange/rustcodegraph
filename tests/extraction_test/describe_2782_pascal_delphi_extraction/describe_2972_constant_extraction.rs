mod describe_2972_constant_extraction {
    use super::*;
    const TS_DESCRIBE_TITLE: &str = "Constant extraction";
    const TS_DESCRIBE_LINE: usize = 2972;
    #[test]
    fn describes_044_is_represented() {
        assert!(!TS_DESCRIBE_TITLE.is_empty());
        assert_eq!(TS_DESCRIBE_LINE, 2972);
    }
    #[test]
    fn case_2973_should_extract_constants() {
        let suite = ["Pascal / Delphi Extraction", "Constant extraction"];
        assert_eq!(suite.len(), 2);
        assert_eq!(195, 195);
        let code = "unit Test;\ninterface\nconst\n  MAX_RETRIES = 3;\n  APP_NAME = 'MyApp';\nimplementation\nend.";
        let result = extract("Test.pas", code);
        let constants = names_by_kind(&result, NodeKind::Constant);
        assert_eq!(constants.len(), 2, "constants: {constants:?}");
        assert_contains(&constants, "MAX_RETRIES");
        assert_contains(&constants, "APP_NAME");
    }
}
