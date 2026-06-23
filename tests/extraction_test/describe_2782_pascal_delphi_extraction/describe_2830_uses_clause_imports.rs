mod describe_2830_uses_clause_imports {
    use super::*;
    const TS_DESCRIBE_TITLE: &str = "Uses clause (imports)";
    const TS_DESCRIBE_LINE: usize = 2830;
    #[test]
    fn describes_037_is_represented() {
        assert!(!TS_DESCRIBE_TITLE.is_empty());
        assert_eq!(TS_DESCRIBE_LINE, 2830);
    }
    #[test]
    fn case_2831_should_extract_uses_as_individual_imports() {
        let suite = ["Pascal / Delphi Extraction", "Uses clause (imports)"];
        assert_eq!(suite.len(), 2);
        assert_eq!(184, 184);
        let code = "unit Test;\ninterface\nuses\n  System.SysUtils,\n  System.Classes;\nimplementation\nend.";
        let result = extract("Test.pas", code);
        assert_import_names(&result, &["System.SysUtils", "System.Classes"]);
    }
    #[test]
    fn case_2841_should_create_unresolved_references_for_imports() {
        let suite = ["Pascal / Delphi Extraction", "Uses clause (imports)"];
        assert_eq!(suite.len(), 2);
        assert_eq!(185, 185);
        let code = "unit Test;\ninterface\nuses\n  UAuth;\nimplementation\nend.";
        let result = extract("Test.pas", code);
        assert_contains(
            &references_by_kind(&result, ReferenceKind::Imports),
            "UAuth",
        );
    }
}
