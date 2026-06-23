mod describe_6548_regression_issue_specific_extraction_fixes {
    use super::*;
    const TS_DESCRIBE_TITLE: &str = "Regression: issue-specific extraction fixes";
    const TS_DESCRIBE_LINE: usize = 6548;
    #[test]
    fn describes_104_is_represented() {
        assert!(!TS_DESCRIBE_TITLE.is_empty());
        assert_eq!(TS_DESCRIBE_LINE, 6548);
    }
    #[test]
    fn case_6549_indexes_inner_functions_of_an_anonymous_amd_commonjs_module_wrapper_52() {
        let suite = ["Regression: issue-specific extraction fixes"];
        assert_eq!(suite.len(), 1);
        assert_eq!(341, 341);
        let code = r#"
define(['dep'], function (dep) {
  function innerHelper(x) { return x + 1; }
  function compute(y) { return innerHelper(y); }
  return { compute: compute };
});
"#;
        let result = extract("amd-module.js", code);
        assert_names_include(&result, NodeKind::Function, &["innerHelper", "compute"]);
    }
    #[test]
    fn case_6563_attaches_go_methods_on_generic_receivers_to_their_type_583() {
        let suite = ["Regression: issue-specific extraction fixes"];
        assert_eq!(suite.len(), 1);
        assert_eq!(342, 342);
        let code = r#"
package main

type Stack[T any] struct { items []T }

func (s *Stack[T]) Push(v T) { s.items = append(s.items, v) }
func (s Stack[T]) Len() int { return len(s.items) }
"#;
        let result = extract("stack.go", code);
        let push = expect_node(&result, NodeKind::Method, "Push");
        let len = expect_node(&result, NodeKind::Method, "Len");
        assert_eq!(push.qualified_name, "Stack::Push");
        assert_eq!(len.qualified_name, "Stack::Len");
    }
    #[test]
    fn case_6578_indexes_new_module_extensions_mts_cts_ts_and_xsjs_xsjslib_js_366_556() {
        let suite = ["Regression: issue-specific extraction fixes"];
        assert_eq!(suite.len(), 1);
        assert_eq!(343, 343);
        assert!(is_source_file("mod.mts"));
        assert!(is_source_file("mod.cts"));
        assert!(is_source_file("service.xsjs"));
        assert!(is_source_file("lib.xsjslib"));
        assert_detected_language("mod.mts", None, Language::TypeScript);
        assert_detected_language("mod.cts", None, Language::TypeScript);
        assert_detected_language("service.xsjs", None, Language::JavaScript);
        assert_detected_language("lib.xsjslib", None, Language::JavaScript);

        let ts = extract("mod.mts", "export function hello(): number { return 1; }");
        expect_node(&ts, NodeKind::Function, "hello");
        let cts = extract("mod.cts", "export function load(): number { return 2; }");
        expect_node(&cts, NodeKind::Function, "load");
        let js = extract("service.xsjs", "function handleRequest() { return 1; }");
        expect_node(&js, NodeKind::Function, "handleRequest");
        let xsjslib = extract("lib.xsjslib", "function helper() { return 2; }");
        expect_node(&xsjslib, NodeKind::Function, "helper");
    }
}
