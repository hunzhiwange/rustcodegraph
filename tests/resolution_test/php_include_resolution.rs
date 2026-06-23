use super::*;

fn mk_php_ref(name: &str, language: Language, reference_kind: ReferenceKind) -> UnresolvedRef {
    UnresolvedRef {
        from_node_id: "f".to_owned(),
        reference_name: name.to_owned(),
        reference_kind,
        line: 1,
        column: 0,
        file_path: "x.php".to_owned(),
        language,
        candidates: None,
    }
}

#[test]
fn is_php_include_path_ref_distinguishes_include_paths_from_namespace_use_660() {
    assert!(is_php_include_path_ref(&mk_php_ref(
        "lib.php",
        Language::Php,
        ReferenceKind::Imports,
    )));
    assert!(is_php_include_path_ref(&mk_php_ref(
        "inc/db.php",
        Language::Php,
        ReferenceKind::Imports,
    )));
    assert!(is_php_include_path_ref(&mk_php_ref(
        "../config.php",
        Language::Php,
        ReferenceKind::Imports,
    )));

    assert!(!is_php_include_path_ref(&mk_php_ref(
        "Closure",
        Language::Php,
        ReferenceKind::Imports,
    )));
    assert!(!is_php_include_path_ref(&mk_php_ref(
        "PDO",
        Language::Php,
        ReferenceKind::Imports,
    )));
    assert!(!is_php_include_path_ref(&mk_php_ref(
        r"App\Foo\Bar",
        Language::Php,
        ReferenceKind::Imports,
    )));
    assert!(!is_php_include_path_ref(&mk_php_ref(
        "lib.php",
        Language::C,
        ReferenceKind::Imports,
    )));
    assert!(!is_php_include_path_ref(&mk_php_ref(
        "lib.php",
        Language::Php,
        ReferenceKind::Calls,
    )));
}

ignored_backend_test!(
    resolves_require_once_to_a_file_to_file_imports_edge_660,
    "resolves require_once to a file->file imports edge (#660)"
);
ignored_backend_test!(
    resolves_a_subdirectory_include_path_to_the_correct_file_660,
    "resolves a subdirectory include path to the correct file (#660)"
);
ignored_backend_test!(
    does_not_mis_connect_an_unresolvable_include_to_same_named_file_elsewhere_660,
    "does not mis-connect an unresolvable include to a same-named file elsewhere (#660)"
);
