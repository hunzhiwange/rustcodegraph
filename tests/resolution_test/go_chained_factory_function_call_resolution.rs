use super::*;

ignored_backend_test!(
    resolves_new_bar_via_the_factory_return_type_pointer_never_decoy,
    "resolves New().Bar() via the factory return type (pointer), never a same-named decoy"
);
ignored_backend_test!(
    resolves_an_args_chain_and_multi_return_factory,
    "resolves an args chain and a multi-return factory - With(c).Build(), (*Foo, error)"
);
ignored_backend_test!(
    resolves_a_method_provided_by_an_embedded_struct,
    "resolves a method provided by an embedded struct (via conformance)"
);
ignored_backend_test!(
    creates_no_edge_when_neither_type_nor_embedded_type_has_the_method,
    "creates NO edge when neither the type nor an embedded type has the method (silent miss)"
);
ignored_backend_test!(
    falls_back_to_bare_name_resolution_for_variable_inner_chain_without_exploding_graph,
    "falls back to bare-name resolution for a VARIABLE-inner chain without exploding the graph"
);
