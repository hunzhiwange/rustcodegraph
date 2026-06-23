mod describe_2945_enum_extraction {
    use super::*;
    const TS_DESCRIBE_TITLE: &str = "Enum extraction";
    const TS_DESCRIBE_LINE: usize = 2945;
    #[test]
    fn describes_042_is_represented() {
        assert!(!TS_DESCRIBE_TITLE.is_empty());
        assert_eq!(TS_DESCRIBE_LINE, 2945);
    }
    #[test]
    fn case_2946_should_extract_enums_with_members() {
        let suite = ["Pascal / Delphi Extraction", "Enum extraction"];
        assert_eq!(suite.len(), 2);
        assert_eq!(193, 193);
        let code = "unit Test;\ninterface\ntype\n  TColor = (clRed, clGreen, clBlue);\nimplementation\nend.";
        let result = extract("Test.pas", code);
        find_node(&result, NodeKind::Enum, "TColor").expect("TColor enum should exist");
        assert_eq!(
            names_by_kind(&result, NodeKind::EnumMember),
            ["clRed", "clGreen", "clBlue"]
        );
    }
}
