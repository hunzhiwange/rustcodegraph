use super::*;

ignored_backend_test!(
    should_create_resolver_from_codegraph_instance,
    "should create resolver from CodeGraph instance"
);

#[test]
fn should_resolve_references_after_indexing() {
    let project = TempProject::new("codegraph-rust-resolution");
    project.mkdir("src");
    project.write(
        "src/helper.ts",
        r#"
export function helperFunction(): void {}
"#,
    );
    project.write(
        "src/main.ts",
        r#"
import { helperFunction as runHelper } from './helper';

function main(): void {
  runHelper();
}
"#,
    );

    let mut cg = CodeGraph::init_sync(project.path()).expect("CodeGraph should initialize");
    let index_result = cg.index_all(IndexOptions::default());
    assert!(index_result.success);
    assert!(
        index_result.edges_created > 0,
        "indexing should run resolver wiring and create edges"
    );
    let _ = cg.resolve_references();

    let helper = cg
        .get_nodes_by_kind(NodeKind::Function)
        .into_iter()
        .find(|node| node.name == "helperFunction" && node.file_path == "src/helper.ts")
        .expect("helper function should be indexed");
    let incoming = cg.get_incoming_edges(&helper.id);
    assert!(
        incoming.iter().any(|edge| {
            edge.kind == EdgeKind::Calls
                && cg
                    .get_node(&edge.source)
                    .map(|node| node.name == "main")
                    .unwrap_or(false)
        }),
        "import-resolved call edge should target helperFunction"
    );

    cg.destroy();
}

ignored_backend_test!(
    promotes_calls_to_instantiates_when_target_resolves_to_a_class_python,
    "promotes calls->instantiates when target resolves to a class (Python)"
);
ignored_backend_test!(
    resolves_a_cross_file_static_method_call_to_the_method_not_the_class_825,
    "resolves a cross-file static method call to the method, not the class (#825)"
);
ignored_backend_test!(
    resolves_go_cross_package_qualified_calls_via_go_mod_module_path_388,
    "resolves Go cross-package qualified calls via go.mod module path (#388)"
);
ignored_backend_test!(
    resolves_go_aliased_imports_across_packages_388,
    "resolves Go aliased imports across packages (#388)"
);
ignored_backend_test!(
    resolves_python_module_attribute_calls_after_from_pkg_import_module_578,
    "resolves Python module-attribute calls after `from pkg import module` (#578)"
);
ignored_backend_test!(
    attaches_go_methods_to_their_receiver_type_across_files_583_cross_file_half,
    "attaches Go methods to their receiver type across files (#583, cross-file half)"
);
ignored_backend_test!(
    ts_type_alias_object_shape_members_resolve_method_calls_359,
    "TS type_alias object-shape members resolve method calls (#359)"
);
ignored_backend_test!(
    java_import_disambiguates_same_name_classes_across_modules_314,
    "Java import disambiguates same-name classes across modules (#314)"
);
ignored_backend_test!(
    csharp_extracts_references_from_method_property_field_types_381,
    "C# extracts references from method/property/field types (#381)"
);
ignored_backend_test!(
    csharp_primary_constructor_parameters_record_their_type_dependencies_237,
    "C# primary-constructor parameters record their type dependencies (#237)"
);
ignored_backend_test!(
    go_leaves_stdlib_calls_external,
    "Go: leaves stdlib calls (fmt.Println, etc.) external"
);
