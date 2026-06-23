use crate::common::*;
use rustcodegraph::resolution::frameworks::index::{
    detect_frameworks, get_all_framework_resolvers,
};
use rustcodegraph::resolution::import_resolver::{
    extract_import_mappings, is_php_include_path_ref, load_cpp_include_dirs, resolve_import_path,
    resolve_jvm_import,
};
use rustcodegraph::resolution::name_matcher::match_reference;
use rustcodegraph::resolution::types::{ResolvedBy, UnresolvedRef};
use rustcodegraph::types::{EdgeKind, Language, Node, NodeKind, ReferenceKind};
use rustcodegraph::{CodeGraph, IndexOptions};

mod c_cpp_import_resolution;
mod chained_call_resolves_a_method_on_a_supertype_conformance_750;
mod cpp_chained_call_receiver_resolution_645;
mod csharp_chained_static_factory_call_resolution;
mod dart_chained_static_factory_and_factory_constructor_call_resolution;
mod framework_detection;
mod go_chained_factory_function_call_resolution;
mod import_resolver;
mod integration_tests;
mod java_chained_static_factory_call_resolution;
mod jvm_fqn_import_resolution;
mod kotlin_chained_companion_factory_call_resolution;
mod name_matcher;
mod name_matcher_kind_bias_for_new_ref_kinds;
mod objective_c_chained_message_send_call_resolution;
mod pascal_delphi_chained_static_factory_call_resolution;
mod php_chained_static_factory_call_resolution_608;
mod php_include_resolution;
mod re_export_chain_following;
mod react_framework_resolver;
mod rust_chained_associated_function_call_resolution;
mod scala_chained_static_factory_call_resolution;
mod swift_chained_static_factory_call_resolution;
mod tsconfig_path_aliases;
