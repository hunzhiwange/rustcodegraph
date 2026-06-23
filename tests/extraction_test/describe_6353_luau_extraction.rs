mod describe_6353_luau_extraction {
    use super::*;
    const TS_DESCRIBE_TITLE: &str = "Luau Extraction";
    const TS_DESCRIBE_LINE: usize = 6353;
    #[test]
    fn describes_098_is_represented() {
        assert!(!TS_DESCRIBE_TITLE.is_empty());
        assert_eq!(TS_DESCRIBE_LINE, 6353);
    }
    mod describe_6354_language_detection {
        use super::*;
        const TS_DESCRIBE_TITLE: &str = "Language detection";
        const TS_DESCRIBE_LINE: usize = 6354;
        #[test]
        fn describes_099_is_represented() {
            assert!(!TS_DESCRIBE_TITLE.is_empty());
            assert_eq!(TS_DESCRIBE_LINE, 6354);
        }
        #[test]
        fn case_6355_should_detect_luau_files() {
            let suite = ["Luau Extraction", "Language detection"];
            assert_eq!(suite.len(), 2);
            assert_eq!(329, 329);
            assert_detected_language("init.luau", None, Language::Luau);
            assert_detected_language("src/Client.luau", None, Language::Luau);
        }
        #[test]
        fn case_6360_should_report_luau_as_supported() {
            let suite = ["Luau Extraction", "Language detection"];
            assert_eq!(suite.len(), 2);
            assert_eq!(330, 330);
            assert_language_support(Language::Luau, true);
            assert_supported_languages_include(&[Language::Luau]);
        }
    }
    mod describe_6366_type_aliases {
        use super::*;
        const TS_DESCRIBE_TITLE: &str = "Type aliases";
        const TS_DESCRIBE_LINE: usize = 6366;
        #[test]
        fn describes_100_is_represented() {
            assert!(!TS_DESCRIBE_TITLE.is_empty());
            assert_eq!(TS_DESCRIBE_LINE, 6366);
        }
        #[test]
        fn case_6367_should_extract_type_and_export_type_definitions() {
            let suite = ["Luau Extraction", "Type aliases"];
            assert_eq!(suite.len(), 2);
            assert_eq!(331, 331);
            let code = r#"
export type Vector = { x: number, y: number }
type Handler = (msg: string) -> boolean
"#;
            let result = extract("types.luau", code);
            let vector = find_node(&result, NodeKind::TypeAlias, "Vector")
                .expect("Vector type alias should be extracted");
            assert!(is_exported(vector));
            let handler = find_node(&result, NodeKind::TypeAlias, "Handler")
                .expect("Handler type alias should be extracted");
            assert!(!is_exported(handler));
        }
    }
    mod describe_6383_typed_functions_and_methods {
        use super::*;
        const TS_DESCRIBE_TITLE: &str = "Typed functions and methods";
        const TS_DESCRIBE_LINE: usize = 6383;
        #[test]
        fn describes_101_is_represented() {
            assert!(!TS_DESCRIBE_TITLE.is_empty());
            assert_eq!(TS_DESCRIBE_LINE, 6383);
        }
        #[test]
        fn case_6384_should_capture_typed_signatures_and_split_methods_by_receiver() {
            let suite = ["Luau Extraction", "Typed functions and methods"];
            assert_eq!(suite.len(), 2);
            assert_eq!(332, 332);
            let code = r#"
function configure(opts: { debug: boolean }): boolean
	return opts.debug
end
function Client:fetch(path: string): Response
	return path
end
"#;
            let result = extract("client.luau", code);
            let configure = find_node(&result, NodeKind::Function, "configure")
                .expect("configure function should be extracted");
            assert_eq!(configure.language, Language::Luau);
            assert_eq!(
                configure.signature.as_deref(),
                Some("(opts: { debug: boolean }): boolean")
            );
            let fetch = find_node(&result, NodeKind::Method, "fetch")
                .expect("fetch method should be extracted");
            assert_eq!(fetch.qualified_name, "Client::fetch");
        }
    }
    mod describe_6402_imports_and_variables {
        use super::*;
        const TS_DESCRIBE_TITLE: &str = "Imports and variables";
        const TS_DESCRIBE_LINE: usize = 6402;
        #[test]
        fn describes_102_is_represented() {
            assert!(!TS_DESCRIBE_TITLE.is_empty());
            assert_eq!(TS_DESCRIBE_LINE, 6402);
        }
        #[test]
        fn case_6403_should_extract_string_and_roblox_instance_path_require_imports() {
            let suite = ["Luau Extraction", "Imports and variables"];
            assert_eq!(suite.len(), 2);
            assert_eq!(333, 333);
            let code = r#"
local http = require("http")
local Signal = require(script.Parent.Signal)
local count = 0
"#;
            let result = extract("mod.luau", code);
            let imports = names_by_kind(&result, NodeKind::Import);
            assert_contains(&imports, "http");
            assert_contains(&imports, "Signal");
            let vars = names_by_kind(&result, NodeKind::Variable);
            assert_contains(&vars, "count");
        }
    }
}
