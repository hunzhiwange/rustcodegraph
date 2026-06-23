use super::*;

ignored_backend_test!(
    resolves_singleton_chains_and_auto_locals_to_the_right_class_never_first_sorted,
    "resolves singleton chains and auto locals to the right class, never the first-sorted one"
);
ignored_backend_test!(
    resolves_factories_free_function_factories_and_member_chains_via_inner_call_return_type,
    "resolves factories, free-function factories, and member chains via the inner call return type"
);
ignored_backend_test!(
    creates_no_edge_when_inferred_type_lacks_the_method,
    "creates NO edge when the inferred type lacks the method (silent miss, not a wrong edge)"
);
