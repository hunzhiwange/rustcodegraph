use super::*;

ignored_backend_test!(
    resolves_chained_message_send_via_return_type_never_decoy,
    "resolves a chained message send [[Foo create] doIt] via the return type, never a same-named decoy"
);
ignored_backend_test!(
    resolves_chained_message_whose_method_is_inherited_from_superclass,
    "resolves a chained message whose method is inherited from a superclass (via conformance)"
);
ignored_backend_test!(
    creates_no_edge_when_factory_return_type_lacks_method_objc,
    "creates NO edge when the factory return type lacks the method (silent miss)"
);
ignored_backend_test!(
    resolves_singleton_chain_whose_factory_returns_nonnull_instancetype,
    "resolves a singleton chain [[Cache shared] clearAll] whose factory returns nonnull instancetype"
);
