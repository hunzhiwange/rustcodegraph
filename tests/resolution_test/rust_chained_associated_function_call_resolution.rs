use super::*;

ignored_backend_test!(
    resolves_foo_new_bar_and_self_return_via_associated_fn_never_decoy,
    "resolves Foo::new().bar() (and a Self return) via the associated fn, never a same-named decoy"
);
ignored_backend_test!(
    resolves_a_chain_that_passes_arguments_foo_with_c_build,
    "resolves a chain that passes arguments - Foo::with(c).build()"
);
ignored_backend_test!(
    resolves_a_chained_method_from_a_trait_the_type_implements,
    "resolves a chained method from a trait the type implements (default method, via conformance)"
);
ignored_backend_test!(
    creates_no_edge_when_neither_type_nor_supertype_has_the_method_rust,
    "creates NO edge when neither the type nor a supertype has the method (silent miss)"
);
