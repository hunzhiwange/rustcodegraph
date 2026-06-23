use super::*;

ignored_backend_test!(
    resolves_companion_factory_chain_to_return_type_never_decoy,
    "resolves a companion-factory chain Foo.create().doIt() to the return type, never a same-named decoy"
);
ignored_backend_test!(
    resolves_case_class_apply_construction_on_constructed_class,
    "resolves a case-class apply construction Point(x).dist() on the constructed class"
);
ignored_backend_test!(
    resolves_chained_method_provided_by_trait_return_type_extends,
    "resolves a chained method provided by a trait the return type extends (via conformance)"
);
ignored_backend_test!(
    creates_no_edge_when_neither_factory_return_type_nor_supertype_has_method_scala,
    "creates NO edge when neither the factory return type nor a supertype has the method (silent miss)"
);
