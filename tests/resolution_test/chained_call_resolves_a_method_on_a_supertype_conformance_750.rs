use super::*;

ignored_backend_test!(
    resolves_a_chained_method_defined_only_on_a_superclass_the_return_type_extends,
    "resolves a chained method defined only on a SUPERCLASS the return type extends"
);
ignored_backend_test!(
    resolves_a_chained_method_defined_on_an_interface_the_return_type_implements,
    "resolves a chained method defined on an INTERFACE the return type implements (default method)"
);
ignored_backend_test!(
    still_creates_no_edge_when_no_supertype_has_the_method,
    "still creates NO edge when no supertype has the method (safety preserved)"
);
