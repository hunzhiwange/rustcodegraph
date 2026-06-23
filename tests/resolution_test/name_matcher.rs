use super::*;

#[test]
fn should_match_exact_name_references() {
    let mock_nodes = vec![test_node(
        "func:test.ts:myFunction:10",
        NodeKind::Function,
        "myFunction",
        "test.ts::myFunction",
        "test.ts",
        Language::TypeScript,
        10,
    )];
    let mut context = MockResolutionContext::with_nodes(mock_nodes).with_file_exists_default(true);

    let reference = unresolved_ref(
        "myFunction",
        ReferenceKind::Calls,
        "main.ts",
        Language::TypeScript,
    );
    let result = match_reference(&reference, &mut context);

    let result = result.expect("reference should resolve");
    assert_eq!(result.target_node_id, "func:test.ts:myFunction:10");
    assert_eq!(result.resolved_by, ResolvedBy::ExactMatch);
}

#[test]
fn should_prefer_same_module_candidates_over_cross_module_matches() {
    let candidate_a = test_node(
        "func:apps/app_a/src/server.py:navigate:10",
        NodeKind::Function,
        "navigate",
        "apps/app_a/src/server.py::navigate",
        "apps/app_a/src/server.py",
        Language::Python,
        10,
    );
    let candidate_b = test_node(
        "func:apps/app_b/src/server.py:navigate:15",
        NodeKind::Function,
        "navigate",
        "apps/app_b/src/server.py::navigate",
        "apps/app_b/src/server.py",
        Language::Python,
        15,
    );
    let mut context = MockResolutionContext::with_nodes(vec![candidate_a, candidate_b])
        .with_file_exists_default(true);

    let reference = unresolved_ref(
        "navigate",
        ReferenceKind::Calls,
        "apps/app_a/src/handler.py",
        Language::Python,
    );
    let result = match_reference(&reference, &mut context);

    let result = result.expect("reference should resolve");
    assert_eq!(
        result.target_node_id,
        "func:apps/app_a/src/server.py:navigate:10"
    );
    assert_eq!(result.resolved_by, ResolvedBy::ExactMatch);
}

#[test]
fn should_lower_confidence_for_cross_module_exact_matches() {
    let candidates = vec![
        test_node(
            "func:apps/app_b/src/server.py:navigate:10",
            NodeKind::Function,
            "navigate",
            "apps/app_b/src/server.py::navigate",
            "apps/app_b/src/server.py",
            Language::Python,
            10,
        ),
        test_node(
            "func:apps/app_c/src/server.py:navigate:10",
            NodeKind::Function,
            "navigate",
            "apps/app_c/src/server.py::navigate",
            "apps/app_c/src/server.py",
            Language::Python,
            10,
        ),
    ];
    let mut context = MockResolutionContext::with_nodes(candidates).with_file_exists_default(true);

    let reference = unresolved_ref(
        "navigate",
        ReferenceKind::Calls,
        "apps/app_a/src/handler.py",
        Language::Python,
    );
    let result = match_reference(&reference, &mut context);

    assert!(
        result.expect("reference should resolve").confidence <= 0.4,
        "cross-module exact match should have low confidence"
    );
}

#[test]
fn should_match_qualified_name_references() {
    let mock_class_node = test_node(
        "class:user.ts:User:5",
        NodeKind::Class,
        "User",
        "user.ts::User",
        "user.ts",
        Language::TypeScript,
        5,
    );
    let mock_method_node = test_node(
        "method:user.ts:User.save:15",
        NodeKind::Method,
        "save",
        "user.ts::User::save",
        "user.ts",
        Language::TypeScript,
        15,
    );
    let mut context = MockResolutionContext::with_nodes(vec![mock_class_node, mock_method_node])
        .with_file_exists_default(true)
        .with_all_files(&["user.ts"]);

    let reference = unresolved_ref(
        "User.save",
        ReferenceKind::Calls,
        "main.ts",
        Language::TypeScript,
    );
    let result = match_reference(&reference, &mut context);

    assert_eq!(
        result
            .expect("qualified reference should resolve")
            .target_node_id,
        "method:user.ts:User.save:15"
    );
}
