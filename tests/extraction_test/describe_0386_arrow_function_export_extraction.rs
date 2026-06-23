mod describe_0386_arrow_function_export_extraction {
    use super::*;
    const TS_DESCRIBE_TITLE: &str = "Arrow Function Export Extraction";
    const TS_DESCRIBE_LINE: usize = 386;
    #[test]
    fn describes_004_is_represented() {
        assert!(!TS_DESCRIBE_TITLE.is_empty());
        assert_eq!(TS_DESCRIBE_LINE, 386);
    }
    #[test]
    fn case_0387_should_extract_exported_arrow_functions_assigned_to_const() {
        let suite = ["Arrow Function Export Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(30, 30);
        let code = r#"
export const useAuth = (): AuthContextValue => {
  return useContext(AuthContext);
};
"#;
        let result = extract("hooks.ts", code);
        let func_node = find_node(&result, NodeKind::Function, "useAuth")
            .expect("useAuth function should be extracted");
        assert!(is_exported(func_node));
    }
    #[test]
    fn case_0404_should_extract_exported_function_expressions_assigned_to_const() {
        let suite = ["Arrow Function Export Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(31, 31);
        let code = r#"
export const processData = function(input: string): string {
  return input.trim();
};
"#;
        let result = extract("utils.ts", code);
        let func_node = find_node(&result, NodeKind::Function, "processData")
            .expect("processData function should be extracted");
        assert!(is_exported(func_node));
    }
    #[test]
    fn case_0421_should_not_extract_non_exported_arrow_functions_as_exported() {
        let suite = ["Arrow Function Export Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(32, 32);
        let code = r#"
const internalHelper = () => {
  return 42;
};
"#;
        let result = extract("internal.ts", code);
        let helper_node = find_node(&result, NodeKind::Function, "internalHelper")
            .expect("internalHelper function should be extracted");
        assert!(!is_exported(helper_node));
    }
    #[test]
    fn case_0434_should_still_skip_truly_anonymous_arrow_functions() {
        let suite = ["Arrow Function Export Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(33, 33);
        let code = r#"
const items = [1, 2, 3].map((x) => x * 2);
"#;
        let result = extract("anon.ts", code);
        assert!(
            !result
                .nodes
                .iter()
                .any(|node| node.kind == NodeKind::Function && node.name == "<anonymous>"),
            "nodes: {:?}",
            result.nodes
        );
    }
    #[test]
    fn case_0448_should_extract_multiple_exported_arrow_functions_from_the_same_file() {
        let suite = ["Arrow Function Export Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(34, 34);
        let code = r#"
export const add = (a: number, b: number): number => a + b;

export const subtract = (a: number, b: number): number => a - b;

const internal = () => 'not exported';
"#;
        let result = extract("math.ts", code);
        let mut exported = result
            .nodes
            .iter()
            .filter(|node| node.kind == NodeKind::Function && is_exported(node))
            .map(|node| node.name.clone())
            .collect::<Vec<_>>();
        exported.sort();
        assert_eq!(exported, vec!["add", "subtract"]);

        let internal = find_node(&result, NodeKind::Function, "internal")
            .expect("internal function should be extracted");
        assert!(!is_exported(internal));
    }
    #[test]
    fn case_0467_should_extract_arrow_functions_in_javascript_files() {
        let suite = ["Arrow Function Export Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(35, 35);
        let code = r#"
export const fetchData = async () => {
  const response = await fetch('/api/data');
  return response.json();
};
"#;
        let result = extract("api.js", code);
        let func_node = find_node(&result, NodeKind::Function, "fetchData")
            .expect("fetchData function should be extracted");
        assert!(is_exported(func_node));
    }
}
