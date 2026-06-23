mod describe_6752_go_cross_package_composite_literals_blast_radius_recall {
    use super::*;
    const TS_DESCRIBE_TITLE: &str = "Go cross-package composite literals (blast-radius recall)";
    const TS_DESCRIBE_LINE: usize = 6752;
    #[test]
    fn describes_107_is_represented() {
        assert!(!TS_DESCRIBE_TITLE.is_empty());
        assert_eq!(TS_DESCRIBE_LINE, 6752);
    }
    #[test]
    fn case_6757_links_a_cross_package_struct_composite_literal_to_the_defining_package() {
        let suite = ["Go cross-package composite literals (blast-radius recall)"];
        assert_eq!(suite.len(), 1);
        assert_eq!(352, 352);
        let temp = TempDir::new("codegraph-go-composite-dependent");
        temp.write("go.mod", "module example.com/proj\n\ngo 1.21\n");
        temp.write(
            "render/xml.go",
            "package render\n\ntype XML struct { Data any }\n",
        );
        temp.write(
            "app.go",
            "package main\n\nimport \"example.com/proj/render\"\n\nfunc handle() any { return render.XML{} }\n",
        );
        let mut cg = index_project(&temp);
        let dependents = cg.get_file_dependents("render/xml.go");
        assert_contains(&dependents, "app.go");
    }
    #[test]
    fn case_6774_links_a_composite_literal_in_a_package_level_var_registry() {
        let suite = ["Go cross-package composite literals (blast-radius recall)"];
        assert_eq!(suite.len(), 1);
        assert_eq!(353, 353);
        let temp = TempDir::new("codegraph-go-registry-dependent");
        temp.write("go.mod", "module example.com/proj\n\ngo 1.21\n");
        temp.write(
            "render/xml.go",
            "package render\n\ntype XML struct {}\nfunc (XML) Render() {}\n",
        );
        temp.write(
            "reg.go",
            "package main\n\nimport \"example.com/proj/render\"\n\ntype R interface { Render() }\n\nvar registry = map[string]R{ \"xml\": render.XML{} }\n",
        );
        let mut cg = index_project(&temp);
        let dependents = cg.get_file_dependents("render/xml.go");
        assert_contains(&dependents, "reg.go");
    }
    #[test]
    fn case_6794_attributes_a_call_inside_a_top_level_closure_cobra_rune_to_the_var_not() {
        let suite = ["Go cross-package composite literals (blast-radius recall)"];
        assert_eq!(suite.len(), 1);
        assert_eq!(354, 354);
        let temp = TempDir::new("codegraph-go-closure-caller");
        temp.write("go.mod", "module example.com/proj\n\ngo 1.21\n");
        temp.write(
            "factory.go",
            "package main\n\nfunc Wire() error { return nil }\n",
        );
        temp.write(
            "root.go",
            "package main\n\ntype Cmd struct{ RunE func() error }\n\nvar rootCmd = &Cmd{\n\tRunE: func() error { return Wire() },\n}\n",
        );
        let mut cg = index_project(&temp);
        let wire = cg
            .search_nodes("Wire", None)
            .into_iter()
            .map(|result| result.node)
            .find(|node| node.kind == NodeKind::Function && node.name == "Wire")
            .expect("Wire function should be indexed");
        let callers = cg
            .get_callers(&wire.id, 1)
            .into_iter()
            .map(|caller| caller.node)
            .collect::<Vec<_>>();
        assert!(
            callers
                .iter()
                .any(|node| node.kind == NodeKind::Variable && node.name == "rootCmd"),
            "expected rootCmd variable caller, got {callers:?}"
        );
        assert!(
            callers.iter().all(|node| node.kind != NodeKind::File),
            "expected no file caller, got {callers:?}"
        );
    }
    #[test]
    fn case_6822_links_a_parenthesized_pointer_type_conversion_t_x_to_the_type() {
        let suite = ["Go cross-package composite literals (blast-radius recall)"];
        assert_eq!(suite.len(), 1);
        assert_eq!(355, 355);
        let temp = TempDir::new("codegraph-go-pointer-conversion");
        temp.write("go.mod", "module example.com/proj\n\ngo 1.21\n");
        temp.write(
            "types.go",
            "package main\n\ntype Wrapped struct { N int }\n",
        );
        temp.write(
            "use.go",
            "package main\n\nfunc run(x *int) { _ = (*Wrapped)(x) }\n",
        );
        let mut cg = index_project(&temp);
        let dependents = cg.get_file_dependents("types.go");
        assert_contains(&dependents, "use.go");
    }
    #[test]
    fn case_6840_links_an_implementation_reached_only_through_a_go_interface_implicit_s() {
        let suite = ["Go cross-package composite literals (blast-radius recall)"];
        assert_eq!(suite.len(), 1);
        assert_eq!(356, 356);
        let temp = TempDir::new("codegraph-go-implicit-interface");
        temp.write("go.mod", "module example.com/proj\n\ngo 1.21\n");
        temp.write(
            "codec/api.go",
            "package codec\n\ntype Core interface {\n\tMarshal(v any) ([]byte, error)\n}\n\nvar API Core\n",
        );
        temp.write(
            "codec/json.go",
            "package codec\n\ntype jsonApi struct{}\n\nfunc (j jsonApi) Marshal(v any) ([]byte, error) { return nil, nil }\n\nfunc init() { API = jsonApi{} }\n",
        );
        let mut cg = index_project(&temp);
        let dependents = cg.get_file_dependents("codec/json.go");
        assert_contains(&dependents, "codec/api.go");
    }
}
