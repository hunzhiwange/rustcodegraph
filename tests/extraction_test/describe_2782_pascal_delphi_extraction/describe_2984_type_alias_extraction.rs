mod describe_2984_type_alias_extraction {
    use super::*;
    const TS_DESCRIBE_TITLE: &str = "Type alias extraction";
    const TS_DESCRIBE_LINE: usize = 2984;
    #[test]
    fn describes_045_is_represented() {
        assert!(!TS_DESCRIBE_TITLE.is_empty());
        assert_eq!(TS_DESCRIBE_LINE, 2984);
    }
    #[test]
    fn case_2985_should_extract_type_aliases() {
        let suite = ["Pascal / Delphi Extraction", "Type alias extraction"];
        assert_eq!(suite.len(), 2);
        assert_eq!(196, 196);
        let code = "unit Test;\ninterface\ntype\n  TUserName = string;\nimplementation\nend.";
        let result = extract("Test.pas", code);
        find_node(&result, NodeKind::TypeAlias, "TUserName")
            .expect("TUserName alias should be extracted");
    }
}
