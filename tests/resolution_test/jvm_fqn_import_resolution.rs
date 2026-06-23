use super::*;

fn jvm_node(
    id: &str,
    name: &str,
    qualified_name: &str,
    kind: NodeKind,
    language: Language,
) -> Node {
    test_node(id, kind, name, qualified_name, "Models.kt", language, 1)
}

fn import_ref(reference_name: &str, language: Language) -> UnresolvedRef {
    UnresolvedRef {
        from_node_id: "caller".to_owned(),
        reference_name: reference_name.to_owned(),
        reference_kind: ReferenceKind::Imports,
        line: 1,
        column: 0,
        file_path: "Caller.kt".to_owned(),
        language,
        candidates: None,
    }
}

#[test]
fn resolves_a_kotlin_class_import_by_fqn_regardless_of_filename() {
    let target = jvm_node(
        "n1",
        "Bar",
        "com.example.foo::Bar",
        NodeKind::Class,
        Language::Kotlin,
    );
    let mut ctx = MockResolutionContext::with_nodes(vec![target]);
    let result = resolve_jvm_import(
        &import_ref("com.example.foo.Bar", Language::Kotlin),
        &mut ctx,
    );

    let result = result.expect("Kotlin class import should resolve");
    assert_eq!(result.target_node_id, "n1");
    assert_eq!(result.resolved_by, ResolvedBy::Import);
}

#[test]
fn resolves_a_kotlin_top_level_function_import_by_fqn() {
    let util = jvm_node(
        "n2",
        "util",
        "com.example.foo::util",
        NodeKind::Function,
        Language::Kotlin,
    );
    let mut ctx = MockResolutionContext::with_nodes(vec![util]);
    let result = resolve_jvm_import(
        &import_ref("com.example.foo.util", Language::Kotlin),
        &mut ctx,
    );

    assert_eq!(
        result.map(|resolved| resolved.target_node_id),
        Some("n2".to_owned())
    );
}

#[test]
fn resolves_a_java_import_by_fqn() {
    let target = jvm_node(
        "n3",
        "Bar",
        "com.example.foo::Bar",
        NodeKind::Class,
        Language::Java,
    );
    let mut ctx = MockResolutionContext::with_nodes(vec![target]);
    let result = resolve_jvm_import(&import_ref("com.example.foo.Bar", Language::Java), &mut ctx);

    assert_eq!(
        result.map(|resolved| resolved.target_node_id),
        Some("n3".to_owned())
    );
}

#[test]
fn resolves_cross_language_kotlin_importing_a_java_class() {
    let target = jvm_node(
        "n4",
        "JavaBar",
        "com.example::JavaBar",
        NodeKind::Class,
        Language::Java,
    );
    let mut ctx = MockResolutionContext::with_nodes(vec![target]);
    let result = resolve_jvm_import(
        &import_ref("com.example.JavaBar", Language::Kotlin),
        &mut ctx,
    );

    assert_eq!(
        result.map(|resolved| resolved.target_node_id),
        Some("n4".to_owned())
    );
}

#[test]
fn disambiguates_a_name_collision_across_packages() {
    let bar_a = jvm_node(
        "n5a",
        "Bar",
        "com.example.alpha::Bar",
        NodeKind::Class,
        Language::Kotlin,
    );
    let bar_b = jvm_node(
        "n5b",
        "Bar",
        "com.example.beta::Bar",
        NodeKind::Class,
        Language::Kotlin,
    );
    let mut ctx = MockResolutionContext::with_nodes(vec![bar_a, bar_b]);

    assert_eq!(
        resolve_jvm_import(
            &import_ref("com.example.alpha.Bar", Language::Kotlin),
            &mut ctx
        )
        .map(|resolved| resolved.target_node_id),
        Some("n5a".to_owned())
    );
    assert_eq!(
        resolve_jvm_import(
            &import_ref("com.example.beta.Bar", Language::Kotlin),
            &mut ctx
        )
        .map(|resolved| resolved.target_node_id),
        Some("n5b".to_owned())
    );
}

#[test]
fn returns_null_for_wildcard_imports() {
    let mut ctx = MockResolutionContext::new();
    assert!(
        resolve_jvm_import(&import_ref("com.example.foo.*", Language::Kotlin), &mut ctx).is_none()
    );
}

#[test]
fn returns_null_for_unqualified_names() {
    let mut ctx = MockResolutionContext::with_nodes(vec![jvm_node(
        "n6",
        "Bar",
        "Bar",
        NodeKind::Class,
        Language::Kotlin,
    )]);
    assert!(resolve_jvm_import(&import_ref("Bar", Language::Kotlin), &mut ctx).is_none());
}

#[test]
fn returns_null_for_non_jvm_languages() {
    let target = jvm_node(
        "n7",
        "Bar",
        "com.example::Bar",
        NodeKind::Class,
        Language::Kotlin,
    );
    let mut ctx = MockResolutionContext::with_nodes(vec![target]);
    assert!(
        resolve_jvm_import(
            &import_ref("com.example.Bar", Language::TypeScript),
            &mut ctx
        )
        .is_none()
    );
}

#[test]
fn returns_null_for_non_import_reference_kinds() {
    let target = jvm_node(
        "n8",
        "Bar",
        "com.example::Bar",
        NodeKind::Class,
        Language::Kotlin,
    );
    let mut ctx = MockResolutionContext::with_nodes(vec![target]);
    let mut reference = import_ref("com.example.Bar", Language::Kotlin);
    reference.reference_kind = ReferenceKind::Calls;

    assert!(resolve_jvm_import(&reference, &mut ctx).is_none());
}

#[test]
fn returns_null_when_the_fqn_is_not_in_the_index() {
    let mut ctx = MockResolutionContext::new();
    assert!(
        resolve_jvm_import(
            &import_ref("com.example.Unknown", Language::Kotlin),
            &mut ctx
        )
        .is_none()
    );
}
