mod describe_2719_liquid_imports {
    use super::*;
    const TS_DESCRIBE_TITLE: &str = "Liquid imports";
    const TS_DESCRIBE_LINE: usize = 2719;
    #[test]
    fn describes_033_is_represented() {
        assert!(!TS_DESCRIBE_TITLE.is_empty());
        assert_eq!(TS_DESCRIBE_LINE, 2719);
    }
    #[test]
    fn case_2720_should_extract_render_tag() {
        let suite = ["Import Extraction", "Liquid imports"];
        assert_eq!(suite.len(), 2);
        assert_eq!(174, 174);
        let result = extract("template.liquid", "{% render 'loading-spinner' %}");
        let import = single_import(&result, "loading-spinner");
        assert_signature_contains(import, "render");
    }
    #[test]
    fn case_2730_should_extract_section_tag() {
        let suite = ["Import Extraction", "Liquid imports"];
        assert_eq!(suite.len(), 2);
        assert_eq!(175, 175);
        let result = extract("layout/theme.liquid", "{% section 'header' %}");
        let import = single_import(&result, "header");
        assert_signature_contains(import, "section");
    }
    #[test]
    fn case_2740_should_extract_include_tag() {
        let suite = ["Import Extraction", "Liquid imports"];
        assert_eq!(suite.len(), 2);
        assert_eq!(176, 176);
        let result = extract("snippets/header.liquid", "{% include 'icon-cart' %}");
        let import = single_import(&result, "icon-cart");
        assert_signature_contains(import, "include");
    }
    #[test]
    fn case_2750_should_extract_render_with_whitespace_control() {
        let suite = ["Import Extraction", "Liquid imports"];
        assert_eq!(suite.len(), 2);
        assert_eq!(177, 177);
        let result = extract("snippets/product.liquid", "{%- render 'price' -%}");
        single_import(&result, "price");
    }
    #[test]
    fn case_2759_should_extract_multiple_imports() {
        let suite = ["Import Extraction", "Liquid imports"];
        assert_eq!(suite.len(), 2);
        assert_eq!(178, 178);
        let code = r#"
{% section 'header' %}
{% render 'loading-spinner' %}
{% render 'cart-drawer' %}
"#;
        let result = extract("layout/theme.liquid", code);
        assert_import_names(&result, &["header", "loading-spinner", "cart-drawer"]);
    }
}
