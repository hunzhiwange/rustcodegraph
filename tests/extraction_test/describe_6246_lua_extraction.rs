mod describe_6246_lua_extraction {
    use super::*;
    const TS_DESCRIBE_TITLE: &str = "Lua Extraction";
    const TS_DESCRIBE_LINE: usize = 6246;
    #[test]
    fn describes_092_is_represented() {
        assert!(!TS_DESCRIBE_TITLE.is_empty());
        assert_eq!(TS_DESCRIBE_LINE, 6246);
    }
    mod describe_6247_language_detection {
        use super::*;
        const TS_DESCRIBE_TITLE: &str = "Language detection";
        const TS_DESCRIBE_LINE: usize = 6247;
        #[test]
        fn describes_093_is_represented() {
            assert!(!TS_DESCRIBE_TITLE.is_empty());
            assert_eq!(TS_DESCRIBE_LINE, 6247);
        }
        #[test]
        fn case_6248_should_detect_lua_files() {
            let suite = ["Lua Extraction", "Language detection"];
            assert_eq!(suite.len(), 2);
            assert_eq!(321, 321);
            assert_detected_language("init.lua", None, Language::Lua);
            assert_detected_language("src/util.lua", None, Language::Lua);
        }
        #[test]
        fn case_6253_should_report_lua_as_supported() {
            let suite = ["Lua Extraction", "Language detection"];
            assert_eq!(suite.len(), 2);
            assert_eq!(322, 322);
            assert_language_support(Language::Lua, true);
            assert_supported_languages_include(&[Language::Lua]);
        }
    }
    mod describe_6259_function_extraction {
        use super::*;
        const TS_DESCRIBE_TITLE: &str = "Function extraction";
        const TS_DESCRIBE_LINE: usize = 6259;
        #[test]
        fn describes_094_is_represented() {
            assert!(!TS_DESCRIBE_TITLE.is_empty());
            assert_eq!(TS_DESCRIBE_LINE, 6259);
        }
        #[test]
        fn case_6260_should_extract_global_and_local_functions() {
            let suite = ["Lua Extraction", "Function extraction"];
            assert_eq!(suite.len(), 2);
            assert_eq!(323, 323);
            let code = r#"
function configure(opts) return opts end
local function helper(x) return x * 2 end
"#;
            let result = extract("init.lua", code);
            let funcs = names_by_kind(&result, NodeKind::Function);
            assert_contains(&funcs, "configure");
            assert_contains(&funcs, "helper");

            let configure = find_node(&result, NodeKind::Function, "configure")
                .expect("configure function should be extracted");
            assert_eq!(configure.language, Language::Lua);
            assert_eq!(configure.signature.as_deref(), Some("(opts)"));
        }
        #[test]
        fn case_6274_should_split_table_method_functions_into_a_receiver_and_method_name() {
            let suite = ["Lua Extraction", "Function extraction"];
            assert_eq!(suite.len(), 2);
            assert_eq!(324, 324);
            let code = r#"
function M.connect(host, port) return host end
function M:send(data) return self end
"#;
            let result = extract("init.lua", code);
            let connect = find_node(&result, NodeKind::Method, "connect")
                .expect("connect method should be extracted");
            assert_eq!(connect.qualified_name, "M::connect");
            let send = find_node(&result, NodeKind::Method, "send")
                .expect("send method should be extracted");
            assert_eq!(send.qualified_name, "M::send");
        }
    }
    mod describe_6288_variable_extraction {
        use super::*;
        const TS_DESCRIBE_TITLE: &str = "Variable extraction";
        const TS_DESCRIBE_LINE: usize = 6288;
        #[test]
        fn describes_095_is_represented() {
            assert!(!TS_DESCRIBE_TITLE.is_empty());
            assert_eq!(TS_DESCRIBE_LINE, 6288);
        }
        #[test]
        fn case_6289_should_extract_local_variable_declarations() {
            let suite = ["Lua Extraction", "Variable extraction"];
            assert_eq!(suite.len(), 2);
            assert_eq!(325, 325);
            let code = r#"
local M = {}
local count = 0
"#;
            let result = extract("mod.lua", code);
            let vars = names_by_kind(&result, NodeKind::Variable);
            assert_contains(&vars, "M");
            assert_contains(&vars, "count");
        }
    }
    mod describe_6301_import_extraction_require {
        use super::*;
        const TS_DESCRIBE_TITLE: &str = "Import extraction (require)";
        const TS_DESCRIBE_LINE: usize = 6301;
        #[test]
        fn describes_096_is_represented() {
            assert!(!TS_DESCRIBE_TITLE.is_empty());
            assert_eq!(TS_DESCRIBE_LINE, 6301);
        }
        #[test]
        fn case_6302_should_extract_require_in_local_declarations_and_bare_calls() {
            let suite = ["Lua Extraction", "Import extraction (require)"];
            assert_eq!(suite.len(), 2);
            assert_eq!(326, 326);
            let code = r#"
local socket = require("socket")
local http = require "resty.http"
require("side.effect")
"#;
            let result = extract("net.lua", code);
            let imports = names_by_kind(&result, NodeKind::Import);
            assert_contains(&imports, "socket");
            assert_contains(&imports, "resty.http");
            assert_contains(&imports, "side.effect");

            let import_refs = references_by_kind(&result, ReferenceKind::Imports);
            assert_contains(&import_refs, "socket");
        }
        #[test]
        fn case_6324_should_keep_extracting_require_across_many_sequential_parses() {
            let suite = ["Lua Extraction", "Import extraction (require)"];
            assert_eq!(suite.len(), 2);
            assert_eq!(327, 327);
            let mut last = extract("f0.lua", "local m = require(\"module.0\")\nreturn m\n");
            for i in 1..8 {
                last = extract(
                    &format!("f{i}.lua"),
                    &format!("local m = require(\"module.{i}\")\nreturn m\n"),
                );
            }
            let imports = names_by_kind(&last, NodeKind::Import);
            assert_contains(&imports, "module.7");
        }
    }
    mod describe_6334_call_extraction {
        use super::*;
        const TS_DESCRIBE_TITLE: &str = "Call extraction";
        const TS_DESCRIBE_LINE: usize = 6334;
        #[test]
        fn describes_097_is_represented() {
            assert!(!TS_DESCRIBE_TITLE.is_empty());
            assert_eq!(TS_DESCRIBE_LINE, 6334);
        }
        #[test]
        fn case_6335_should_record_intra_file_calls_as_resolvable_references() {
            let suite = ["Lua Extraction", "Call extraction"];
            assert_eq!(suite.len(), 2);
            assert_eq!(328, 328);
            let code = r#"
local function helper(x) return x end
local function run(y) return helper(y) end
"#;
            let result = extract("calls.lua", code);
            let calls = references_by_kind(&result, ReferenceKind::Calls);
            assert_contains(&calls, "helper");
        }
    }
}
