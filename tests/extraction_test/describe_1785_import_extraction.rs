mod describe_1785_import_extraction {
    use super::*;
    const TS_DESCRIBE_TITLE: &str = "Import Extraction";
    const TS_DESCRIBE_LINE: usize = 1785;
    #[test]
    fn describes_017_is_represented() {
        assert!(!TS_DESCRIBE_TITLE.is_empty());
        assert_eq!(TS_DESCRIBE_LINE, 1785);
    }
    include!("describe_1785_import_extraction/describe_1786_typescript_javascript_imports.rs");
    include!("describe_1785_import_extraction/describe_1889_python_imports.rs");
    include!("describe_1785_import_extraction/describe_1966_rust_imports.rs");
    include!("describe_1785_import_extraction/describe_2018_go_imports.rs");
    include!("describe_1785_import_extraction/describe_2096_swift_imports.rs");
    include!("describe_1785_import_extraction/describe_2144_kotlin_imports.rs");
    include!("describe_1785_import_extraction/describe_2193_java_imports.rs");
    include!("describe_1785_import_extraction/describe_2251_c_imports.rs");
    include!("describe_1785_import_extraction/describe_2309_php_imports.rs");
    include!("describe_1785_import_extraction/describe_2406_ruby_imports.rs");
    include!("describe_1785_import_extraction/describe_2462_ruby_modules.rs");
    include!("describe_1785_import_extraction/describe_2526_php_return_type_capture_608.rs");
    include!("describe_1785_import_extraction/describe_2550_c_c_return_type_capture_645.rs");
    include!("describe_1785_import_extraction/describe_2585_c_c_imports.rs");
    include!("describe_1785_import_extraction/describe_2662_dart_imports.rs");
    include!("describe_1785_import_extraction/describe_2719_liquid_imports.rs");
}
