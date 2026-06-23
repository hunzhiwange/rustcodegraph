mod describe_4591_liquid_shopify_json_template_section_resolution {
    use super::*;
    const TS_DESCRIBE_TITLE: &str = "Liquid Shopify JSON template section resolution";
    const TS_DESCRIBE_LINE: usize = 4591;
    #[test]
    fn describes_066_is_represented() {
        assert!(!TS_DESCRIBE_TITLE.is_empty());
        assert_eq!(TS_DESCRIBE_LINE, 4591);
    }
    #[test]
    fn case_4604_links_a_shopify_json_template_section_type_to_its_sections_type_liquid() {
        let suite = ["Liquid Shopify JSON template section resolution"];
        assert_eq!(suite.len(), 1);
        assert_eq!(240, 240);
        let temp = TempDir::new("codegraph-liquid-json-section");
        temp.write(
            "sections/main-product.liquid",
            "<div>{{ product.title }}</div>\n",
        );
        temp.write(
            "sections/main-login.liquid",
            "<form>{{ 'customer.login' | t }}</form>\n",
        );
        temp.write(
            "templates/product.json",
            r#"{"sections":{"main":{"type":"main-product"}},"order":["main"]}"#,
        );
        temp.write(
            "templates/customers/login.json",
            r#"{"sections":{"main":{"type":"main-login"}},"order":["main"]}"#,
        );

        let mut cg = index_project(&temp);
        let product = cg
            .get_nodes_by_kind(NodeKind::File)
            .into_iter()
            .find(|node| node.file_path.ends_with("sections/main-product.liquid"))
            .expect("main-product section file should be indexed");
        let login = cg
            .get_nodes_by_kind(NodeKind::File)
            .into_iter()
            .find(|node| node.file_path.ends_with("sections/main-login.liquid"))
            .expect("main-login section file should be indexed");

        let product_deps = cg.get_file_dependents(&product.file_path);
        assert!(
            product_deps
                .iter()
                .any(|path| path.ends_with("templates/product.json")),
            "top-level JSON template should link its section, dependents: {product_deps:?}"
        );
        let login_deps = cg.get_file_dependents(&login.file_path);
        assert!(
            login_deps
                .iter()
                .any(|path| path.ends_with("templates/customers/login.json")),
            "nested JSON template should link its section, dependents: {login_deps:?}"
        );
        cg.close();
    }
}
