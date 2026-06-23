//! Extraction tests.
//!
//! Rust port inventory for `__tests__/extraction.test.ts`. The parser-heavy cases below are
//! generated one-to-one from the TypeScript describe/test tree and are ignored
//! until Rust native tree-sitter extraction reaches parity. Each ignored wrapper
//! can still run the original Vitest assertion path for its exact case.

#[path = "extraction_test/common.rs"]
mod common;
use common::*;

include!("extraction_test/inventory.rs");
include!("extraction_test/rust_native_surface_checks.rs");
include!("extraction_test/describe_0033_language_detection.rs");
include!("extraction_test/describe_0110_language_support.rs");
include!("extraction_test/describe_0135_typescript_extraction.rs");
include!("extraction_test/describe_0386_arrow_function_export_extraction.rs");
include!("extraction_test/describe_0486_type_alias_extraction.rs");
include!("extraction_test/describe_0592_exported_variable_extraction.rs");
include!("extraction_test/describe_0721_file_node_extraction.rs");
include!("extraction_test/describe_0769_python_extraction.rs");
include!("extraction_test/describe_0809_go_extraction.rs");
include!("extraction_test/describe_0846_rust_extraction.rs");
include!("extraction_test/describe_0964_java_extraction.rs");
include!("extraction_test/describe_1127_c_extraction.rs");
include!("extraction_test/describe_1268_php_extraction.rs");
include!("extraction_test/describe_1326_swift_extraction.rs");
include!("extraction_test/describe_1442_kotlin_extraction.rs");
include!("extraction_test/describe_1648_dart_extraction.rs");
include!("extraction_test/describe_1785_import_extraction.rs");
include!("extraction_test/describe_2782_pascal_delphi_extraction.rs");
include!("extraction_test/describe_3292_dfm_fmx_extraction.rs");
include!("extraction_test/describe_3490_kotlin_multiplatform_expect_actual.rs");
include!("extraction_test/describe_3627_scala_cross_file_dependencies.rs");
include!("extraction_test/describe_3708_php_namespace_import_resolution.rs");
include!("extraction_test/describe_3794_ruby_mixins_include_extend_prepend.rs");
include!("extraction_test/describe_3900_c_free_function_name_extraction.rs");
include!("extraction_test/describe_3963_dart_mixins_and_type_references.rs");
include!("extraction_test/describe_4035_static_member_value_read_references.rs");
include!("extraction_test/describe_4116_cross_language_type_import_gate_rn_name_collisions.rs");
include!("extraction_test/describe_4205_python_absolute_module_import_resolution.rs");
include!("extraction_test/describe_4302_razor_blazor_markup_extraction.rs");
include!("extraction_test/describe_4419_default_import_resolution_renamed_default_export.rs");
include!("extraction_test/describe_4452_chained_method_call_resolution_c_extension_methods.rs");
include!("extraction_test/describe_4493_same_directory_include_kmp_import_resolution.rs");
include!("extraction_test/describe_4559_delphi_form_code_behind_pairing.rs");
include!("extraction_test/describe_4591_liquid_shopify_json_template_section_resolution.rs");
include!("extraction_test/describe_4629_lua_luau_require_resolution.rs");
include!("extraction_test/describe_4670_rust_module_path_call_resolution.rs");
include!("extraction_test/describe_4777_sveltekit_load_page_synthesizer.rs");
include!("extraction_test/describe_4820_nuxt_nested_auto_imported_component_resolution.rs");
include!("extraction_test/describe_4857_swift_property_wrapper_attribute_type_references.rs");
include!("extraction_test/describe_4893_objective_c_messages_class_receivers_and_import.rs");
include!("extraction_test/describe_4967_full_indexing.rs");
include!("extraction_test/describe_5166_path_normalization.rs");
include!("extraction_test/describe_5181_directory_exclusion.rs");
include!("extraction_test/describe_5269_git_submodules.rs");
include!("extraction_test/describe_5320_nested_non_submodule_git_repos.rs");
include!("extraction_test/describe_5443_scala_extraction.rs");
include!("extraction_test/describe_5709_vue_extraction.rs");
include!("extraction_test/describe_5932_astro_extraction.rs");
include!("extraction_test/describe_6144_instantiates_decorates_edge_extraction.rs");
include!("extraction_test/describe_6246_lua_extraction.rs");
include!("extraction_test/describe_6353_luau_extraction.rs");
include!("extraction_test/describe_6423_objective_c_extraction.rs");
include!("extraction_test/describe_6548_regression_issue_specific_extraction_fixes.rs");
include!(
    "extraction_test/describe_6594_import_re_export_dependency_linking_blast_radius_recall.rs"
);
include!("extraction_test/describe_6671_python_import_dependency_linking_blast_radius_recall.rs");
include!(
    "extraction_test/describe_6752_go_cross_package_composite_literals_blast_radius_recall.rs"
);
include!("extraction_test/describe_6861_c_records_blast_radius_recall.rs");
include!("extraction_test/describe_6893_rust_cross_module_recall.rs");
include!("extraction_test/describe_6974_java_annotations_blast_radius_recall.rs");
include!("extraction_test/describe_6995_swift_property_wrappers_attributes_blast_radius_recall.rs");
include!("extraction_test/describe_7014_r_extraction.rs");
