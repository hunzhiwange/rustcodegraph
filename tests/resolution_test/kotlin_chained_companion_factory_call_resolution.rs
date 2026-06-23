use super::*;

ignored_backend_test!(
    resolves_foo_getinstance_bar_via_the_companion_return_type_never_decoy,
    "resolves Foo.getInstance().bar() via the companion return type, never a same-named decoy"
);
ignored_backend_test!(
    resolves_a_companion_factory_chain_that_passes_arguments,
    "resolves a companion factory chain that passes arguments - Foo.create(cfg).build()"
);
ignored_backend_test!(
    creates_no_edge_when_the_companion_return_type_lacks_the_method,
    "creates NO edge when the companion return type lacks the method (silent miss, not a wrong edge)"
);
