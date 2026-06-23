mod describe_0486_type_alias_extraction {
    use super::*;
    const TS_DESCRIBE_TITLE: &str = "Type Alias Extraction";
    const TS_DESCRIBE_LINE: usize = 486;
    #[test]
    fn describes_005_is_represented() {
        assert!(!TS_DESCRIBE_TITLE.is_empty());
        assert_eq!(TS_DESCRIBE_LINE, 486);
    }
    #[test]
    fn case_0487_should_extract_exported_type_aliases_in_typescript() {
        let suite = ["Type Alias Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(36, 36);
        let code = r#"
export type AuthContextValue = {
  user: User | null;
  login: () => void;
  logout: () => void;
};
"#;
        let result = extract("types.ts", code);
        let type_node = find_node(&result, NodeKind::TypeAlias, "AuthContextValue")
            .expect("AuthContextValue type alias should be extracted");
        assert!(is_exported(type_node));
    }
    #[test]
    fn case_0505_should_extract_non_exported_type_aliases() {
        let suite = ["Type Alias Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(37, 37);
        let code = r#"
type InternalState = {
  loading: boolean;
  error: string | null;
};
"#;
        let result = extract("internal.ts", code);
        let type_node = find_node(&result, NodeKind::TypeAlias, "InternalState")
            .expect("InternalState type alias should be extracted");
        assert!(!is_exported(type_node));
    }
    #[test]
    fn case_0522_should_extract_multiple_type_aliases_from_the_same_file() {
        let suite = ["Type Alias Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(38, 38);
        let code = r#"
export type UnitSystem = 'metric' | 'imperial';
export type DateFormat = 'ISO' | 'US' | 'EU';
type Internal = string;
"#;
        let result = extract("config.ts", code);
        let type_aliases = nodes_by_kind(&result, NodeKind::TypeAlias);
        assert_eq!(type_aliases.len(), 3, "nodes: {:?}", result.nodes);

        let mut exported = type_aliases
            .iter()
            .filter(|node| is_exported(node))
            .map(|node| node.name.clone())
            .collect::<Vec<_>>();
        exported.sort();
        assert_eq!(exported, vec!["DateFormat", "UnitSystem"]);
    }
    #[test]
    fn case_0541_extracts_string_literal_contract_names_from_a_generic_tuple_type_alias() {
        let suite = ["Type Alias Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(39, 39);
        let code = r#"
interface Service<Name extends string, Req, Resp> { name: Name; }
export type MyServiceList = [
  Service<'query_apply_record', { pageNo: number }, { ok: boolean }>,
  Service<'apply_confirm', { code: string }, { ok: boolean }>
];
"#;
        let result = extract("services/api.ts", code);

        let mut names = result
            .nodes
            .iter()
            .filter(|node| {
                node.kind == NodeKind::Method && node.qualified_name.starts_with("MyServiceList::")
            })
            .map(|node| node.name.clone())
            .collect::<Vec<_>>();
        names.sort();
        assert_eq!(names, vec!["apply_confirm", "query_apply_record"]);

        let query_node = result
            .nodes
            .iter()
            .find(|node| node.kind == NodeKind::Method && node.name == "query_apply_record")
            .expect("query_apply_record contract method should be extracted");
        assert_eq!(
            query_node.qualified_name,
            "MyServiceList::query_apply_record"
        );
        assert!(
            query_node
                .signature
                .as_deref()
                .is_some_and(|signature| signature.contains("Service<'query_apply_record'")),
            "signature: {:?}",
            query_node.signature
        );

        let alias = find_node(&result, NodeKind::TypeAlias, "MyServiceList")
            .expect("MyServiceList type alias should be extracted");
        assert!(
            result.edges.iter().any(|edge| {
                edge.kind == EdgeKind::Contains
                    && edge.source == alias.id
                    && edge.target == query_node.id
            }),
            "contains edges: {:?}",
            result.edges
        );
    }
    #[test]
    fn case_0569_does_not_extract_string_literals_from_utility_types_or_nested_generics() {
        let suite = ["Type Alias Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(40, 40);
        let code = r#"
interface User { id: string; name: string; }
interface Service<Name extends string, Req, Resp> { name: Name; }
export type Picked = Pick<User, 'id' | 'name'>;
export type Rec = Record<'foo' | 'bar', number>;
// Tuple entry, but the name is a non-identifier route path; the nested Pick's
// 'id' must also stay out (only DIRECT literal args of a tuple's generic count).
export type Routes = [Service<'/api/users', Pick<User, 'id'>, {}>];
// Bare string-literal tuple - not generic type arguments.
export type Names = ['alpha', 'beta'];
"#;
        let result = extract("noise.ts", code);
        let leaked = result
            .nodes
            .iter()
            .filter(|node| matches!(node.kind, NodeKind::Method | NodeKind::Property))
            .filter(|node| {
                ["id", "name", "foo", "bar", "alpha", "beta"].contains(&node.name.as_str())
            })
            .collect::<Vec<_>>();
        assert!(leaked.is_empty(), "leaked string-literal nodes: {leaked:?}");
    }
}
