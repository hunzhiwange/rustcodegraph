use super::*;

#[test]
fn should_resolve_react_component_references() {
    let mock_nodes = vec![test_node(
        "component:src/Button.tsx:Button:5",
        NodeKind::Component,
        "Button",
        "src/Button.tsx::Button",
        "src/Button.tsx",
        Language::Tsx,
        5,
    )];
    let mut context = MockResolutionContext::with_nodes(mock_nodes)
        .with_file_contents(&[("package.json", r#"{"dependencies":{"react":"^18.0.0"}}"#)])
        .with_all_files(&["package.json", "src/Button.tsx", "src/App.tsx"])
        .with_project_root("/test");

    let frameworks = detect_frameworks(&mut context);
    let react_resolver = frameworks
        .iter()
        .find(|resolver| resolver.name() == "react")
        .expect("React resolver should be detected");

    let reference = unresolved_ref(
        "Button",
        ReferenceKind::References,
        "src/App.tsx",
        Language::Tsx,
    );
    let result = react_resolver.resolve(&reference, &mut context);

    assert_eq!(
        result
            .expect("component reference should resolve")
            .target_node_id,
        "component:src/Button.tsx:Button:5"
    );

    let ts_ref = unresolved_ref(
        "Button",
        ReferenceKind::References,
        "src/models.ts",
        Language::TypeScript,
    );
    assert!(react_resolver.resolve(&ts_ref, &mut context).is_none());
}

#[test]
fn should_resolve_custom_hook_references() {
    let mock_nodes = vec![test_node(
        "hook:src/hooks/useAuth.ts:useAuth:1",
        NodeKind::Function,
        "useAuth",
        "src/hooks/useAuth.ts::useAuth",
        "src/hooks/useAuth.ts",
        Language::TypeScript,
        1,
    )];
    let mut context = MockResolutionContext::with_nodes(mock_nodes)
        .with_file_contents(&[("package.json", r#"{"dependencies":{"react":"^18.0.0"}}"#)])
        .with_all_files(&["package.json", "src/hooks/useAuth.ts"])
        .with_project_root("/test");

    let frameworks = detect_frameworks(&mut context);
    let react_resolver = frameworks
        .iter()
        .find(|resolver| resolver.name() == "react")
        .expect("React resolver should be detected");

    let reference = unresolved_ref(
        "useAuth",
        ReferenceKind::Calls,
        "src/App.tsx",
        Language::TypeScript,
    );
    let result = react_resolver.resolve(&reference, &mut context);

    assert_eq!(
        result
            .expect("hook reference should resolve")
            .target_node_id,
        "hook:src/hooks/useAuth.ts:useAuth:1"
    );
}
