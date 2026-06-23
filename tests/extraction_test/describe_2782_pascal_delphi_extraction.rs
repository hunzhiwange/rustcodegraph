mod describe_2782_pascal_delphi_extraction {
    use super::*;
    const TS_DESCRIBE_TITLE: &str = "Pascal / Delphi Extraction";
    const TS_DESCRIBE_LINE: usize = 2782;
    #[test]
    fn describes_034_is_represented() {
        assert!(!TS_DESCRIBE_TITLE.is_empty());
        assert_eq!(TS_DESCRIBE_LINE, 2782);
    }
    include!("describe_2782_pascal_delphi_extraction/describe_2783_language_detection.rs");
    include!("describe_2782_pascal_delphi_extraction/describe_2799_unit_extraction.rs");
    include!("describe_2782_pascal_delphi_extraction/describe_2830_uses_clause_imports.rs");
    include!("describe_2782_pascal_delphi_extraction/describe_2853_class_extraction.rs");
    include!("describe_2782_pascal_delphi_extraction/describe_2889_record_extraction.rs");
    include!("describe_2782_pascal_delphi_extraction/describe_2905_interface_extraction.rs");
    include!("describe_2782_pascal_delphi_extraction/describe_2916_method_extraction.rs");
    include!("describe_2782_pascal_delphi_extraction/describe_2945_enum_extraction.rs");
    include!("describe_2782_pascal_delphi_extraction/describe_2960_property_extraction.rs");
    include!("describe_2782_pascal_delphi_extraction/describe_2972_constant_extraction.rs");
    include!("describe_2782_pascal_delphi_extraction/describe_2984_type_alias_extraction.rs");
    include!("describe_2782_pascal_delphi_extraction/describe_2995_call_extraction.rs");
    include!("describe_2782_pascal_delphi_extraction/describe_3008_containment_edges.rs");
    include!("describe_2782_pascal_delphi_extraction/describe_3025_full_fixture_uauth_pas.rs");
    include!("describe_2782_pascal_delphi_extraction/describe_3165_full_fixture_utypes_pas.rs");
}
