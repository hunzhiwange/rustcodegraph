mod describe_0592_exported_variable_extraction {
    use super::*;
    const TS_DESCRIBE_TITLE: &str = "Exported Variable Extraction";
    const TS_DESCRIBE_LINE: usize = 592;
    #[test]
    fn describes_006_is_represented() {
        assert!(!TS_DESCRIBE_TITLE.is_empty());
        assert_eq!(TS_DESCRIBE_LINE, 592);
    }
    #[test]
    fn case_0593_should_extract_exported_const_with_call_expression_zustand_store() {
        let suite = ["Exported Variable Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(41, 41);
        let code = r#"
export const useUIStore = create<UIState>((set) => ({
  isOpen: false,
  toggle: () => set((s) => ({ isOpen: !s.isOpen })),
}));
"#;
        let result = extract("store.ts", code);
        let var_node = find_node(&result, NodeKind::Constant, "useUIStore")
            .expect("useUIStore constant should be extracted");
        assert!(is_exported(var_node));
    }
    #[test]
    fn case_0607_should_extract_exported_const_with_object_literal() {
        let suite = ["Exported Variable Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(42, 42);
        let code = r#"
export const config = {
  apiUrl: 'https://api.example.com',
  timeout: 5000,
};
"#;
        let result = extract("config.ts", code);
        let var_node = find_node(&result, NodeKind::Constant, "config")
            .expect("config constant should be extracted");
        assert!(is_exported(var_node));
    }
    #[test]
    fn case_0621_should_extract_exported_const_with_array_literal() {
        let suite = ["Exported Variable Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(43, 43);
        let code = r#"
export const SCREEN_NAMES = ['home', 'settings', 'profile'] as const;
"#;
        let result = extract("constants.ts", code);
        let var_node = find_node(&result, NodeKind::Constant, "SCREEN_NAMES")
            .expect("SCREEN_NAMES constant should be extracted");
        assert!(is_exported(var_node));
    }
    #[test]
    fn case_0632_should_extract_exported_const_with_primitive_value() {
        let suite = ["Exported Variable Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(44, 44);
        let code = r#"
export const MAX_RETRIES = 3;
export const API_VERSION = "v2";
"#;
        let result = extract("constants.ts", code);
        let mut variables = names_by_kind(&result, NodeKind::Constant);
        variables.sort();
        assert_eq!(variables, vec!["API_VERSION", "MAX_RETRIES"]);
    }
    #[test]
    fn case_0644_should_not_duplicate_arrow_functions_as_both_function_and_variable() {
        let suite = ["Exported Variable Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(45, 45);
        let code = r#"
export const useAuth = () => {
  return useContext(AuthContext);
};
"#;
        let result = extract("hooks.ts", code);
        assert_eq!(
            result
                .nodes
                .iter()
                .filter(|node| node.kind == NodeKind::Function && node.name == "useAuth")
                .count(),
            1
        );
        assert_eq!(
            result
                .nodes
                .iter()
                .filter(|node| node.kind == NodeKind::Variable && node.name == "useAuth")
                .count(),
            0
        );
    }
    #[test]
    fn case_0659_should_extract_non_exported_const_as_non_exported_variable() {
        let suite = ["Exported Variable Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(46, 46);
        let code = r#"
const internalConfig = {
  debug: true,
};
"#;
        let result = extract("internal.ts", code);
        let var_nodes = result
            .nodes
            .iter()
            .filter(|node| {
                matches!(node.kind, NodeKind::Variable | NodeKind::Constant)
                    && node.name == "internalConfig"
            })
            .collect::<Vec<_>>();
        assert_eq!(var_nodes.len(), 1, "nodes: {:?}", result.nodes);
        assert!(!is_exported(var_nodes[0]));
    }
    #[test]
    fn case_0673_should_extract_zod_schema_exports() {
        let suite = ["Exported Variable Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(47, 47);
        let code = r#"
export const userSchema = z.object({
  id: z.string(),
  name: z.string(),
  email: z.string().email(),
});
"#;
        let result = extract("schemas.ts", code);
        let var_node = find_node(&result, NodeKind::Constant, "userSchema")
            .expect("userSchema constant should be extracted");
        assert!(is_exported(var_node));
    }
    #[test]
    fn case_0688_should_extract_xstate_machine_exports() {
        let suite = ["Exported Variable Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(48, 48);
        let code = r#"
export const authMachine = createMachine({
  id: "auth",
  initial: "idle",
  states: {
    idle: {},
    authenticated: {},
  },
});
"#;
        let result = extract("machine.ts", code);
        let var_node = find_node(&result, NodeKind::Constant, "authMachine")
            .expect("authMachine constant should be extracted");
        assert!(is_exported(var_node));
    }
    #[test]
    fn case_0706_should_extract_calls_from_a_top_level_variable_initializer_issue_425() {
        let suite = ["Exported Variable Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(49, 49);
        let code = r#"
import { getTokenMp } from './api/upload';

const token = getTokenMp();
"#;
        let result = extract("app.ts", code);
        let calls = references_by_kind(&result, ReferenceKind::Calls);
        assert_contains(&calls, "getTokenMp");
    }
}
