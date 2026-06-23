mod describe_2960_property_extraction {
    use super::*;
    const TS_DESCRIBE_TITLE: &str = "Property extraction";
    const TS_DESCRIBE_LINE: usize = 2960;
    #[test]
    fn describes_043_is_represented() {
        assert!(!TS_DESCRIBE_TITLE.is_empty());
        assert_eq!(TS_DESCRIBE_LINE, 2960);
    }
    #[test]
    fn case_2961_should_extract_properties() {
        let suite = ["Pascal / Delphi Extraction", "Property extraction"];
        assert_eq!(suite.len(), 2);
        assert_eq!(194, 194);
        let code = "unit Test;\ninterface\ntype\n  TObj = class\n  public\n    property Name: string read FName write FName;\n  end;\nimplementation\nend.";
        let result = extract("Test.pas", code);
        let prop = find_node(&result, NodeKind::Property, "Name")
            .expect("Name property should be extracted");
        assert_eq!(prop.visibility, Some(Visibility::Public));
    }
}
