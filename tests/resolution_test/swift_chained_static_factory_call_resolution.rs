use super::*;

ignored_backend_test!(
    resolves_foo_make_draw_via_the_factory_return_type_never_decoy,
    "resolves Foo.make().draw() via the factory return type, never a same-named decoy"
);
ignored_backend_test!(
    resolves_constructor_chain_and_args_factory_chain_swift,
    "resolves a constructor chain Foo().draw() and an args factory chain Foo.build(c).render()"
);
ignored_backend_test!(
    creates_no_edge_when_the_factory_return_type_lacks_the_method_swift,
    "creates NO edge when the factory return type lacks the method (silent miss, not a wrong edge)"
);
