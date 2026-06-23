use super::*;

ignored_backend_test!(
    resolves_cls_for_x_method_via_the_factorys_self_return_608,
    "resolves Cls::for($x)->method() via the factory's `: self` return (#608)"
);
ignored_backend_test!(
    creates_no_edge_when_the_factory_result_lacks_the_method_608,
    "creates NO edge when the factory result lacks the method (#608)"
);
