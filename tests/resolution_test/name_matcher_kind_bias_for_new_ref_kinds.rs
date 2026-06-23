use super::*;

#[test]
fn prefers_a_class_candidate_over_a_function_for_instantiates_refs() {
    let function = test_node(
        "func:utils.ts:Logger:5",
        NodeKind::Function,
        "Logger",
        "utils.ts::Logger",
        "utils.ts",
        Language::TypeScript,
        5,
    );
    let class = test_node(
        "class:logger.ts:Logger:10",
        NodeKind::Class,
        "Logger",
        "logger.ts::Logger",
        "logger.ts",
        Language::TypeScript,
        10,
    );
    let mut context =
        MockResolutionContext::with_nodes(vec![function, class]).with_file_exists_default(true);
    let reference = unresolved_ref(
        "Logger",
        ReferenceKind::Instantiates,
        "main.ts",
        Language::TypeScript,
    );

    let result = match_reference(&reference, &mut context);

    assert_eq!(
        result
            .expect("instantiates ref should resolve")
            .target_node_id,
        "class:logger.ts:Logger:10"
    );
}

#[test]
fn prefers_a_function_candidate_over_a_non_function_for_decorates_refs() {
    let variable = test_node(
        "var:config.ts:Inject:5",
        NodeKind::Variable,
        "Inject",
        "config.ts::Inject",
        "config.ts",
        Language::TypeScript,
        5,
    );
    let decorator = test_node(
        "func:di.ts:Inject:10",
        NodeKind::Function,
        "Inject",
        "di.ts::Inject",
        "di.ts",
        Language::TypeScript,
        10,
    );
    let mut context =
        MockResolutionContext::with_nodes(vec![variable, decorator]).with_file_exists_default(true);
    let reference = unresolved_ref(
        "Inject",
        ReferenceKind::Decorates,
        "svc.ts",
        Language::TypeScript,
    );

    let result = match_reference(&reference, &mut context);

    assert_eq!(
        result.expect("decorates ref should resolve").target_node_id,
        "func:di.ts:Inject:10"
    );
}
