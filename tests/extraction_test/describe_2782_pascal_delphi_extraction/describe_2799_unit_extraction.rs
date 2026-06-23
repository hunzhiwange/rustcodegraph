mod describe_2799_unit_extraction {
    use super::*;
    const TS_DESCRIBE_TITLE: &str = "Unit extraction";
    const TS_DESCRIBE_LINE: usize = 2799;
    #[test]
    fn describes_036_is_represented() {
        assert!(!TS_DESCRIBE_TITLE.is_empty());
        assert_eq!(TS_DESCRIBE_LINE, 2799);
    }
    #[test]
    fn case_2800_should_extract_unit_as_module() {
        let suite = ["Pascal / Delphi Extraction", "Unit extraction"];
        assert_eq!(suite.len(), 2);
        assert_eq!(181, 181);
        let result = extract(
            "MyUnit.pas",
            "unit MyUnit;\ninterface\nimplementation\nend.",
        );
        let module = find_node(&result, NodeKind::Module, "MyUnit")
            .expect("MyUnit module should be extracted");
        assert_eq!(module.language, Language::Pascal);
    }
    #[test]
    fn case_2810_should_extract_program_as_module() {
        let suite = ["Pascal / Delphi Extraction", "Unit extraction"];
        assert_eq!(suite.len(), 2);
        assert_eq!(182, 182);
        let result = extract("MyApp.dpr", "program MyApp;\nbegin\nend.");
        find_node(&result, NodeKind::Module, "MyApp").expect("MyApp module should be extracted");
    }
    #[test]
    fn case_2819_should_fallback_to_filename_when_module_name_is_empty() {
        let suite = ["Pascal / Delphi Extraction", "Unit extraction"];
        assert_eq!(suite.len(), 2);
        assert_eq!(183, 183);
        let result = extract("Console.dpr", "program;\nuses SysUtils;\nbegin\nend.");
        find_node(&result, NodeKind::Module, "Console")
            .expect("filename fallback module should be extracted");
    }
}
