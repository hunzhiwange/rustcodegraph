mod arrow_function_body_traversal {
    use super::*;

    #[test]
    fn should_extract_typescript_function_declaration_with_native_parser() {
        before_all_init_grammars();
        clear_parser_cache();

        let result = extract_from_source(
            "math.ts",
            "export function add(a: number, b: number): number { return a + b; }",
            None,
            None,
        );

        assert!(result.errors.is_empty(), "errors: {:?}", result.errors);
        assert!(
            result
                .nodes
                .iter()
                .any(|node| node.kind == NodeKind::Function && node.name == "add"),
            "nodes: {:?}",
            result
                .nodes
                .iter()
                .map(|node| (&node.kind, node.name.as_str()))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn should_extract_unresolved_references_from_arrow_function_bodies() {
        let code = r#"
export const useAuth = () => {
  const user = getUser();
  const token = generateToken(user);
  return { user, token };
};
"#;
        let result = extract_from_source("hooks.ts", code, None, None);

        let func_node = result
            .nodes
            .iter()
            .find(|node| node.kind == NodeKind::Function && node.name == "useAuth");
        assert!(func_node.is_some());

        let call_names = result
            .unresolved_references
            .iter()
            .filter(|reference| reference.reference_kind == ReferenceKind::Calls)
            .map(|reference| reference.reference_name.as_str())
            .collect::<Vec<_>>();
        assert!(call_names.contains(&"getUser"));
        assert!(call_names.contains(&"generateToken"));
    }

    #[test]
    fn should_extract_unresolved_references_from_function_expression_bodies() {
        let code = r#"
export const processData = function(input: string): string {
  const cleaned = sanitize(input);
  return transform(cleaned);
};
"#;
        let result = extract_from_source("utils.ts", code, None, None);

        let func_node = result
            .nodes
            .iter()
            .find(|node| node.kind == NodeKind::Function && node.name == "processData");
        assert!(func_node.is_some());

        let call_names = result
            .unresolved_references
            .iter()
            .filter(|reference| reference.reference_kind == ReferenceKind::Calls)
            .map(|reference| reference.reference_name.as_str())
            .collect::<Vec<_>>();
        assert!(call_names.contains(&"sanitize"));
        assert!(call_names.contains(&"transform"));
    }

    #[test]
    fn should_not_create_duplicate_nodes_for_arrow_functions() {
        let code = r#"
export const handler = () => {
  doSomething();
};
"#;
        let result = extract_from_source("handler.ts", code, None, None);

        let func_nodes = result
            .nodes
            .iter()
            .filter(|node| node.name == "handler" && node.kind == NodeKind::Function)
            .count();
        let var_nodes = result
            .nodes
            .iter()
            .filter(|node| node.name == "handler" && node.kind == NodeKind::Variable)
            .count();
        assert_eq!(func_nodes, 1);
        assert_eq!(var_nodes, 0);
    }

    #[test]
    fn should_extract_nested_calls_in_arrow_functions_in_javascript() {
        let code = r#"
export const fetchData = async () => {
  const response = await fetchAPI('/data');
  return parseResponse(response);
};
"#;
        let result = extract_from_source("api.js", code, None, None);

        let func_node = result.nodes.iter().find(|node| node.name == "fetchData");
        assert!(func_node.is_some());
        assert_eq!(func_node.map(|node| node.kind), Some(NodeKind::Function));

        let call_names = result
            .unresolved_references
            .iter()
            .filter(|reference| reference.reference_kind == ReferenceKind::Calls)
            .map(|reference| reference.reference_name.as_str())
            .collect::<Vec<_>>();
        assert!(call_names.contains(&"fetchAPI"));
        assert!(call_names.contains(&"parseResponse"));
    }
}
