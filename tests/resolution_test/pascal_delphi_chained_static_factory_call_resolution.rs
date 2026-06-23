use super::*;

ignored_backend_test!(
    resolves_chained_factory_call_via_return_type_never_decoy_pascal,
    "resolves a chained factory call TFoo.GetInstance().DoIt() via the return type, never a same-named decoy"
);
ignored_backend_test!(
    resolves_constructor_chain_on_constructed_class_pascal,
    "resolves a constructor chain TFoo.Create().Configure() on the constructed class"
);
ignored_backend_test!(
    resolves_typecast_chain_on_cast_type_pascal,
    "resolves a typecast chain TFoo(x).DoIt() on the cast type"
);
ignored_backend_test!(
    creates_no_edge_when_factory_return_type_lacks_method_pascal,
    "creates NO edge when the factory return type lacks the method (silent miss)"
);
ignored_backend_test!(
    extracts_paren_less_method_calls,
    "extracts paren-less method calls (Pascal lets a no-arg method drop its parens)"
);
ignored_backend_test!(
    resolves_paren_less_chained_factory_call_via_return_type,
    "resolves a PAREN-LESS chained factory call TFoo.GetInstance.DoIt via the return type"
);
ignored_backend_test!(
    does_not_turn_property_write_read_into_call_edge,
    "does NOT turn a property write/read into a call edge (only statement-level dots are calls)"
);
ignored_backend_test!(
    attributes_implementation_only_free_procedure_calls_to_procedure_not_file,
    "attributes an implementation-only free procedure's calls to the procedure, not the file"
);
