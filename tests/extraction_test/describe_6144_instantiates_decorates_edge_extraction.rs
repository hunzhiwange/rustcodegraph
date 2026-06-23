mod describe_6144_instantiates_decorates_edge_extraction {
    use super::*;
    const TS_DESCRIBE_TITLE: &str = "Instantiates + Decorates edge extraction";
    const TS_DESCRIBE_LINE: usize = 6144;
    #[test]
    fn describes_091_is_represented() {
        assert!(!TS_DESCRIBE_TITLE.is_empty());
        assert_eq!(TS_DESCRIBE_LINE, 6144);
    }
    #[test]
    fn case_6145_emits_an_instantiates_ref_for_new_foo() {
        let suite = ["Instantiates + Decorates edge extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(315, 315);
        let code = "\nclass Foo {}\nfunction bootstrap() { return new Foo(); }\n";
        let result = extract("app.ts", code);
        assert_reference_names_include(&result, ReferenceKind::Instantiates, &["Foo"]);
    }
    #[test]
    fn case_6157_strips_type_argument_suffix_from_generic_constructors() {
        let suite = ["Instantiates + Decorates edge extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(316, 316);
        let code = "\nclass Container<T> { constructor(_: T) {} }\nfunction go() { return new Container<string>('x'); }\n";
        let result = extract("app.ts", code);
        let refs = reference_names(&result, ReferenceKind::Instantiates);
        assert_eq!(refs, vec!["Container".to_owned()]);
    }
    #[test]
    fn case_6172_keeps_trailing_identifier_from_qualified_new_ns_foo() {
        let suite = ["Instantiates + Decorates edge extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(317, 317);
        let code = "\nconst ns = { Foo: class {} };\nfunction go() { return new ns.Foo(); }\n";
        let result = extract("app.ts", code);
        let refs = reference_names(&result, ReferenceKind::Instantiates);
        assert_eq!(refs, vec!["Foo".to_owned()]);
    }
    #[test]
    fn case_6186_emits_a_decorates_ref_for_foo_class_x() {
        let suite = ["Instantiates + Decorates edge extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(318, 318);
        let code =
            "\nfunction Foo(_arg: string) { return (cls: any) => cls; }\n@Foo('x')\nclass X {}\n";
        let result = extract("app.ts", code);
        assert_reference_names_include(&result, ReferenceKind::Decorates, &["Foo"]);
    }
    #[test]
    fn case_6199_does_not_attribute_a_prior_class_s_decorator_to_the_next_class() {
        let suite = ["Instantiates + Decorates edge extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(319, 319);
        let code = "\nfunction A(cls: any) { return cls; }\nfunction B(cls: any) { return cls; }\n@A\nclass Foo {}\n@B\nclass Bar {}\n";
        let result = extract("app.ts", code);
        let decorates = result
            .unresolved_references
            .iter()
            .filter(|reference| reference.reference_kind == ReferenceKind::Decorates)
            .collect::<Vec<_>>();
        let from_bar = decorates
            .iter()
            .filter(|reference| {
                result
                    .nodes
                    .iter()
                    .any(|node| node.id == reference.from_node_id && node.name == "Bar")
            })
            .collect::<Vec<_>>();
        assert_eq!(from_bar.len(), 1, "decorates refs: {decorates:?}");
        assert_eq!(from_bar[0].reference_name, "B");
    }
    #[test]
    fn case_6224_emits_a_decorates_ref_for_foo_method() {
        let suite = ["Instantiates + Decorates edge extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(320, 320);
        let code = "\nfunction Get(p: string) { return (t: any, k: string) => t; }\nclass Svc {\n  @Get('/x') method() { return 1; }\n}\n";
        let result = extract("app.ts", code);
        let decor_method = result
            .unresolved_references
            .iter()
            .find(|reference| {
                reference.reference_kind == ReferenceKind::Decorates
                    && reference.reference_name == "Get"
            })
            .expect("method decorator should be extracted");
        let decorated = result
            .nodes
            .iter()
            .find(|node| node.id == decor_method.from_node_id)
            .expect("decorated node should exist");
        assert_eq!(decorated.name, "method");
    }
}
