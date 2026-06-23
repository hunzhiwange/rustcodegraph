use super::*;

ignored_backend_test!(
    resolves_static_factory_chain_to_return_type_never_decoy,
    "resolves a static-factory chain Foo.makeBar().doIt() to the return type, never a same-named decoy"
);
ignored_backend_test!(
    resolves_named_factory_constructor_chain_on_constructed_class,
    "resolves a named factory-constructor chain Foo.create().ship() on the constructed class"
);
ignored_backend_test!(
    resolves_constructor_receiver_chain_on_constructed_class,
    "resolves a constructor-receiver chain Bar().doIt() on the constructed class"
);
ignored_backend_test!(
    resolves_chained_method_inherited_from_superclass_return_type_extends,
    "resolves a chained method inherited from a superclass the return type extends (via conformance)"
);
ignored_backend_test!(
    creates_no_edge_when_neither_factory_return_type_nor_supertype_has_method_dart,
    "creates NO edge when neither the factory return type nor a supertype has the method (silent miss)"
);
ignored_backend_test!(
    still_extracts_a_method_tree_sitter_misparses_as_constructor,
    "still extracts a method tree-sitter misparses as a constructor (@override + record return)"
);
ignored_backend_test!(
    keeps_plain_construction_as_instantiation_not_constructor_method_call,
    "keeps plain construction Foo() as instantiation, not a Foo::Foo method call"
);
