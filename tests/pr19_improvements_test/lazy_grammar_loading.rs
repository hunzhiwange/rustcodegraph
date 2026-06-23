mod lazy_grammar_loading {
    use super::*;

    #[test]
    fn should_load_grammars_lazily_on_first_use() {
        before_all_init_grammars();
        clear_parser_cache();

        let parser = get_parser(Language::TypeScript);
        assert!(parser.is_some());
    }

    #[test]
    fn should_cache_loaded_grammars() {
        before_all_init_grammars();
        clear_parser_cache();

        let parser1 = get_parser(Language::TypeScript);
        let parser2 = get_parser(Language::TypeScript);

        assert!(parser1.is_some());
        assert!(parser2.is_some());
    }

    #[test]
    fn should_return_null_for_unknown_language() {
        before_all_init_grammars();

        let parser = get_parser(Language::Unknown);
        assert!(parser.is_none());
        clear_parser_cache();
    }

    #[test]
    fn should_handle_unavailable_grammars_gracefully() {
        before_all_init_grammars();

        assert!(!is_language_supported(Language::Unknown));
        clear_parser_cache();
    }

    #[test]
    fn should_report_liquid_as_supported_custom_extractor() {
        before_all_init_grammars();

        assert!(is_language_supported(Language::Liquid));
        clear_parser_cache();
    }

    #[test]
    fn should_include_liquid_in_supported_languages() {
        before_all_init_grammars();

        let supported = get_supported_languages();
        assert!(supported.contains(&Language::Liquid));
        clear_parser_cache();
    }

    #[test]
    fn should_return_unavailable_grammar_errors_as_a_record() {
        before_all_init_grammars();
        clear_parser_cache();

        let errors: HashMap<String, String> = get_unavailable_grammar_errors();
        assert!(
            errors.is_empty() || errors.values().all(|message| !message.is_empty()),
            "errors should be a string record: {errors:?}"
        );
    }

    #[test]
    fn should_support_multiple_languages_independently() {
        before_all_init_grammars();
        clear_parser_cache();

        let ts_parser = get_parser(Language::TypeScript);
        let py_parser = get_parser(Language::Python);

        assert!(ts_parser.is_some());
        assert!(py_parser.is_some());
    }

    #[test]
    fn should_parse_typescript_with_native_tree_sitter() {
        before_all_init_grammars();
        clear_parser_cache();

        let mut parser = get_parser(Language::TypeScript).expect("typescript parser should load");
        let tree = parser
            .parse("export function hello() { return 1; }", None)
            .expect("typescript parser should return a tree");

        assert_eq!(tree.root_node.node_type(), "program");
        assert_eq!(
            tree.root_node
                .descendants_of_type(&["function_declaration"])
                .len(),
            1
        );
    }

    #[test]
    fn should_parse_core_languages_with_native_tree_sitter() {
        before_all_init_grammars();
        clear_parser_cache();

        let cases = [
            (
                Language::TypeScript,
                "export function hello(): number { return 1; }",
                "function_declaration",
            ),
            (
                Language::JavaScript,
                "export function hello() { return 1; }",
                "function_declaration",
            ),
            (
                Language::Rust,
                "pub fn hello() -> i32 { 1 }",
                "function_item",
            ),
            (
                Language::Go,
                "package main\nfunc hello() int { return 1 }\n",
                "function_declaration",
            ),
            (
                Language::Python,
                "def hello():\n    return 1\n",
                "function_definition",
            ),
            (
                Language::Java,
                "class Hello { int hello() { return 1; } }",
                "method_declaration",
            ),
        ];

        for (language, source, expected_node_type) in cases {
            let mut parser =
                get_parser(language).unwrap_or_else(|| panic!("{language:?} parser should load"));
            let tree = parser
                .parse(source, None)
                .unwrap_or_else(|| panic!("{language:?} parser should return a tree"));

            assert!(
                !tree.root_node.has_error,
                "{language:?} parse should not have syntax errors: {:?}",
                tree.root_node
            );
            assert_eq!(
                tree.root_node
                    .descendants_of_type(&[expected_node_type])
                    .len(),
                1,
                "{language:?} should contain {expected_node_type}"
            );
        }
    }

    #[test]
    fn should_clear_all_caches_on_clear_parser_cache() {
        before_all_init_grammars();

        let _ = get_parser(Language::TypeScript);
        clear_parser_cache();

        let errors = get_unavailable_grammar_errors();
        assert!(errors.is_empty());
    }
}
