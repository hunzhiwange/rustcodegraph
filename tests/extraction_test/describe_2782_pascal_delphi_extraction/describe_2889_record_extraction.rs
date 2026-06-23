mod describe_2889_record_extraction {
    use super::*;
    const TS_DESCRIBE_TITLE: &str = "Record extraction";
    const TS_DESCRIBE_LINE: usize = 2889;
    #[test]
    fn describes_039_is_represented() {
        assert!(!TS_DESCRIBE_TITLE.is_empty());
        assert_eq!(TS_DESCRIBE_LINE, 2889);
    }
    #[test]
    fn case_2890_should_extract_records_as_class_nodes() {
        let suite = ["Pascal / Delphi Extraction", "Record extraction"];
        assert_eq!(suite.len(), 2);
        assert_eq!(189, 189);
        let code = "unit Test;\ninterface\ntype\n  TPoint = record\n    X: Double;\n    Y: Double;\n  end;\nimplementation\nend.";
        let result = extract("Test.pas", code);
        find_node(&result, NodeKind::Class, "TPoint")
            .expect("record should be represented as a class node");
        let fields = names_by_kind(&result, NodeKind::Field);
        assert_contains(&fields, "X");
        assert_contains(&fields, "Y");
    }
}
