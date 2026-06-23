//! Tests for the Drupal framework resolver.
//!
//! Rust port of `__tests__/drupal.test.ts`.

use std::collections::HashMap;
use std::fs;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Once;
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};
use std::time::{SystemTime, UNIX_EPOCH};

use rustcodegraph::extraction::grammars::{init_grammars, load_all_grammars};
use rustcodegraph::resolution::frameworks::drupal::DRUPAL_RESOLVER;
use rustcodegraph::resolution::types::{
    FrameworkResolver, ImportMapping, ResolutionContext, UnresolvedRef, now_ms,
};
use rustcodegraph::types::{Language, Node, NodeKind, ReferenceKind};
use rustcodegraph::{CodeGraph, IndexOptions};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

#[derive(Default)]
struct MockResolutionContext {
    nodes: Vec<Node>,
    file_contents: HashMap<String, String>,
    all_files: Vec<String>,
    project_root: String,
}

impl MockResolutionContext {
    fn new() -> Self {
        Self {
            project_root: "/project".to_string(),
            ..Self::default()
        }
    }

    fn with_nodes(mut self, nodes: Vec<Node>) -> Self {
        self.nodes = nodes;
        self
    }

    fn with_file_contents(mut self, entries: &[(&str, &str)]) -> Self {
        for (file_path, content) in entries {
            self.file_contents
                .insert((*file_path).to_string(), (*content).to_string());
        }
        self
    }

    fn with_all_files(mut self, files: &[&str]) -> Self {
        self.all_files = files.iter().map(|file| (*file).to_string()).collect();
        self
    }
}

impl ResolutionContext for MockResolutionContext {
    fn get_nodes_in_file(&mut self, file_path: &str) -> Vec<Node> {
        self.nodes
            .iter()
            .filter(|node| node.file_path == file_path)
            .cloned()
            .collect()
    }

    fn get_nodes_by_name(&mut self, name: &str) -> Vec<Node> {
        self.nodes
            .iter()
            .filter(|node| node.name == name)
            .cloned()
            .collect()
    }

    fn get_nodes_by_qualified_name(&mut self, qualified_name: &str) -> Vec<Node> {
        self.nodes
            .iter()
            .filter(|node| node.qualified_name == qualified_name)
            .cloned()
            .collect()
    }

    fn get_nodes_by_kind(&mut self, kind: NodeKind) -> Vec<Node> {
        self.nodes
            .iter()
            .filter(|node| node.kind == kind)
            .cloned()
            .collect()
    }

    fn file_exists(&mut self, file_path: &str) -> bool {
        self.file_contents.contains_key(file_path)
            || self.nodes.iter().any(|node| node.file_path == file_path)
    }

    fn read_file(&mut self, file_path: &str) -> Option<String> {
        self.file_contents.get(file_path).cloned()
    }

    fn get_project_root(&self) -> String {
        self.project_root.clone()
    }

    fn get_all_files(&mut self) -> Vec<String> {
        if !self.all_files.is_empty() {
            return self.all_files.clone();
        }

        let mut files = self.file_contents.keys().cloned().collect::<Vec<_>>();
        for node in &self.nodes {
            if !files.iter().any(|file| file == &node.file_path) {
                files.push(node.file_path.clone());
            }
        }
        files.sort();
        files
    }

    fn get_nodes_by_lower_name(&mut self, lower_name: &str) -> Vec<Node> {
        self.nodes
            .iter()
            .filter(|node| node.name.to_ascii_lowercase() == lower_name)
            .cloned()
            .collect()
    }

    fn get_import_mappings(&mut self, _file_path: &str, _language: Language) -> Vec<ImportMapping> {
        Vec::new()
    }
}

fn make_context() -> MockResolutionContext {
    MockResolutionContext::new()
}

#[allow(clippy::too_many_arguments)]
fn node(
    id: &str,
    kind: NodeKind,
    name: &str,
    qualified_name: &str,
    file_path: &str,
    language: Language,
    start_line: u64,
    end_line: u64,
) -> Node {
    Node {
        id: id.to_string(),
        kind,
        name: name.to_string(),
        qualified_name: qualified_name.to_string(),
        file_path: file_path.to_string(),
        language,
        start_line,
        end_line,
        start_column: 0,
        end_column: 0,
        docstring: None,
        signature: None,
        visibility: None,
        is_exported: None,
        is_async: None,
        is_static: None,
        is_abstract: None,
        decorators: None,
        type_parameters: None,
        return_type: None,
        updated_at: now_ms(),
    }
}

fn unresolved_ref(reference_name: &str, file_path: &str, language: Language) -> UnresolvedRef {
    UnresolvedRef {
        from_node_id: "route:x".to_string(),
        reference_name: reference_name.to_string(),
        reference_kind: ReferenceKind::References,
        line: 1,
        column: 0,
        file_path: file_path.to_string(),
        language,
        candidates: None,
    }
}

fn noop_raw_waker() -> RawWaker {
    fn clone(_: *const ()) -> RawWaker {
        noop_raw_waker()
    }
    fn wake(_: *const ()) {}
    fn wake_by_ref(_: *const ()) {}
    fn drop(_: *const ()) {}

    static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, wake, wake_by_ref, drop);
    RawWaker::new(std::ptr::null(), &VTABLE)
}

fn block_on<F: Future>(future: F) -> F::Output {
    let waker = unsafe { Waker::from_raw(noop_raw_waker()) };
    let mut cx = Context::from_waker(&waker);
    let mut future = Pin::from(Box::new(future));

    loop {
        match future.as_mut().poll(&mut cx) {
            Poll::Ready(value) => return value,
            Poll::Pending => std::thread::yield_now(),
        }
    }
}

static GRAMMAR_INIT: Once = Once::new();

fn before_all_init_grammars() {
    GRAMMAR_INIT.call_once(|| {
        let _ = block_on(init_grammars());
        let _ = block_on(load_all_grammars());
    });
}

struct TempProject {
    root: PathBuf,
}

impl TempProject {
    fn new(prefix: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after the Unix epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("{prefix}-{}-{nanos}", std::process::id()));
        fs::create_dir_all(&root)
            .unwrap_or_else(|err| panic!("failed to create {}: {err}", root.display()));
        Self { root }
    }

    fn path(&self) -> &Path {
        &self.root
    }

    fn mkdir(&self, relative_path: &str) {
        let path = self.root.join(relative_path);
        fs::create_dir_all(&path)
            .unwrap_or_else(|err| panic!("failed to create {}: {err}", path.display()));
    }

    fn write(&self, relative_path: &str, content: &str) {
        let path = self.root.join(relative_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .unwrap_or_else(|err| panic!("failed to create {}: {err}", parent.display()));
        }
        fs::write(&path, content)
            .unwrap_or_else(|err| panic!("failed to write {}: {err}", path.display()));
    }
}

impl Drop for TempProject {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

// ---------------------------------------------------------------------------
// detect()
// ---------------------------------------------------------------------------

// describe('drupalResolver.detect')
mod drupal_resolver_detect {
    use super::*;

    // it('returns true when composer.json has a drupal/ dependency')
    #[test]
    fn returns_true_when_composer_json_has_a_drupal_dependency() {
        let mut ctx = make_context().with_file_contents(&[(
            "composer.json",
            r#"{"require":{"drupal/core-recommended":"~10.5","drush/drush":"^13"}}"#,
        )]);
        assert!(DRUPAL_RESOLVER.detect(&mut ctx));
    }

    // it('returns true when drupal/ dependency is in require-dev')
    #[test]
    fn returns_true_when_drupal_dependency_is_in_require_dev() {
        let mut ctx = make_context()
            .with_file_contents(&[("composer.json", r#"{"require-dev":{"drupal/core":"^10"}}"#)]);
        assert!(DRUPAL_RESOLVER.detect(&mut ctx));
    }

    // it('returns false when composer.json has no drupal/ dependencies')
    #[test]
    fn returns_false_when_composer_json_has_no_drupal_dependencies() {
        let mut ctx = make_context().with_file_contents(&[(
            "composer.json",
            r#"{"require":{"laravel/framework":"^10","php":">=8.1"}}"#,
        )]);
        assert!(!DRUPAL_RESOLVER.detect(&mut ctx));
    }

    // it('returns false when composer.json is absent')
    #[test]
    fn returns_false_when_composer_json_is_absent() {
        let mut ctx = make_context();
        assert!(!DRUPAL_RESOLVER.detect(&mut ctx));
    }

    // it('returns false when composer.json is malformed JSON')
    #[test]
    fn returns_false_when_composer_json_is_malformed_json() {
        let mut ctx = make_context().with_file_contents(&[("composer.json", "{ bad json")]);
        assert!(!DRUPAL_RESOLVER.detect(&mut ctx));
    }

    // it('returns true for a contrib module with empty require (composer name/type)')
    #[test]
    fn returns_true_for_a_contrib_module_with_empty_require_composer_name_type() {
        let mut ctx = make_context().with_file_contents(&[(
            "composer.json",
            r#"{"name":"drupal/admin_toolbar","type":"drupal-module","require":{}}"#,
        )]);
        assert!(DRUPAL_RESOLVER.detect(&mut ctx));
    }

    // it('returns true via the *.info.yml fallback when composer.json is absent')
    #[test]
    fn returns_true_via_info_yml_fallback_when_composer_json_is_absent() {
        let mut ctx = make_context().with_all_files(&[
            "mymodule/mymodule.info.yml",
            "mymodule/mymodule.routing.yml",
        ]);
        assert!(DRUPAL_RESOLVER.detect(&mut ctx));
    }

    // it('returns false for a stray *.info.yml with no Drupal PHP/route file')
    #[test]
    fn returns_false_for_a_stray_info_yml_with_no_drupal_php_route_file() {
        let mut ctx = make_context().with_all_files(&["some/unrelated.info.yml"]);
        assert!(!DRUPAL_RESOLVER.detect(&mut ctx));
    }
}

// describe('drupalResolver.claimsReference')
mod drupal_resolver_claims_reference {
    use super::*;

    // it('claims FQCN handler refs and hook names the pre-filter would drop')
    #[test]
    fn claims_fqcn_handler_refs_and_hook_names_the_pre_filter_would_drop() {
        assert!(DRUPAL_RESOLVER.claims_reference("\\Drupal\\m\\Form\\SettingsForm"));
        assert!(DRUPAL_RESOLVER.claims_reference("\\Drupal\\m\\Controller\\C:setNoJsCookie"));
        assert!(DRUPAL_RESOLVER.claims_reference("hook_form_alter"));
    }

    // it('does not claim ordinary identifiers or entity-handler dotted refs')
    #[test]
    fn does_not_claim_ordinary_identifiers_or_entity_handler_dotted_refs() {
        assert!(!DRUPAL_RESOLVER.claims_reference("someHelperFunction"));
        assert!(!DRUPAL_RESOLVER.claims_reference("comment.default"));
    }
}

// ---------------------------------------------------------------------------
// extract() - routing.yml
// ---------------------------------------------------------------------------

// describe('drupalResolver.extract - routing.yml')
mod drupal_resolver_extract_routing_yml {
    use super::*;

    const ROUTING: &str = r#"
mymodule.example:
  path: '/mymodule/example'
  defaults:
    _controller: '\Drupal\mymodule\Controller\MyController::build'
    _title: 'Example page'
  requirements:
    _permission: 'access content'
"#;

    // it('emits a route node for each YAML route')
    #[test]
    fn emits_a_route_node_for_each_yaml_route() {
        let result = DRUPAL_RESOLVER.extract("mymodule/mymodule.routing.yml", ROUTING);
        assert_eq!(result.nodes.len(), 1);
        assert_eq!(result.nodes[0].kind, NodeKind::Route);
        assert_eq!(result.nodes[0].name, "/mymodule/example");
    }

    // it('sets qualifiedName to filePath::routeName')
    #[test]
    fn sets_qualified_name_to_file_path_route_name() {
        let result = DRUPAL_RESOLVER.extract("mymodule/mymodule.routing.yml", ROUTING);
        assert_eq!(
            result.nodes[0].qualified_name,
            "mymodule/mymodule.routing.yml::mymodule.example"
        );
    }

    // it('emits a references edge to the controller FQCN')
    #[test]
    fn emits_a_references_edge_to_the_controller_fqcn() {
        let result = DRUPAL_RESOLVER.extract("mymodule/mymodule.routing.yml", ROUTING);
        assert_eq!(result.references.len(), 1);
        assert_eq!(
            result.references[0].reference_name,
            "\\Drupal\\mymodule\\Controller\\MyController::build"
        );
        assert_eq!(
            result.references[0].reference_kind,
            ReferenceKind::References
        );
    }

    // it('emits a references edge to a _form handler')
    #[test]
    fn emits_a_references_edge_to_a_form_handler() {
        let src = r#"
mymodule.settings_form:
  path: '/admin/config/mymodule'
  defaults:
    _form: '\Drupal\mymodule\Form\SettingsForm'
    _title: 'MyModule settings'
  requirements:
    _permission: 'administer site configuration'
"#;
        let result = DRUPAL_RESOLVER.extract("mymodule/mymodule.routing.yml", src);
        assert_eq!(result.nodes.len(), 1);
        assert_eq!(
            result.references[0].reference_name,
            "\\Drupal\\mymodule\\Form\\SettingsForm"
        );
    }

    // it('handles multiple routes in one file')
    #[test]
    fn handles_multiple_routes_in_one_file() {
        let src = r#"
mod.page_one:
  path: '/page-one'
  defaults:
    _controller: '\Drupal\mod\Controller\PageController::one'
  requirements:
    _permission: 'access content'

mod.page_two:
  path: '/page-two'
  defaults:
    _controller: '\Drupal\mod\Controller\PageController::two'
  requirements:
    _permission: 'access content'
"#;
        let result = DRUPAL_RESOLVER.extract("mod/mod.routing.yml", src);
        assert_eq!(result.nodes.len(), 2);
        let names = result
            .nodes
            .iter()
            .map(|node| node.name.as_str())
            .collect::<Vec<_>>();
        assert!(names.contains(&"/page-one"));
        assert!(names.contains(&"/page-two"));
        assert_eq!(result.references.len(), 2);
    }

    // it('skips commented-out lines')
    #[test]
    fn skips_commented_out_lines() {
        let src = r#"
mod.page:
  path: '/page'
  defaults:
    #_controller: '\Drupal\mod\Controller\Old::build'
    _controller: '\Drupal\mod\Controller\New::build'
  requirements:
    _permission: 'access content'
"#;
        let result = DRUPAL_RESOLVER.extract("mod/mod.routing.yml", src);
        assert_eq!(result.references.len(), 1);
        assert!(result.references[0].reference_name.contains("New"));
    }

    // it('includes HTTP methods in the route node name when present')
    #[test]
    fn includes_http_methods_in_the_route_node_name_when_present() {
        let src = r#"
mod.api:
  path: '/api/resource'
  defaults:
    _controller: '\Drupal\mod\Controller\ApiController::get'
  methods: [GET, POST]
  requirements:
    _permission: 'access content'
"#;
        let result = DRUPAL_RESOLVER.extract("mod/mod.routing.yml", src);
        assert!(result.nodes[0].name.contains("GET"));
        assert!(result.nodes[0].name.contains("POST"));
    }

    // it('returns empty result for non-routing-yml files')
    #[test]
    fn returns_empty_result_for_non_routing_yml_files() {
        let result = DRUPAL_RESOLVER.extract("mymodule.module", "<?php\n");
        // Module files go through hook detection, not route extraction.
        assert_eq!(result.nodes.len(), 0);
    }

    // it('returns empty result for files with no valid routes')
    #[test]
    fn returns_empty_result_for_files_with_no_valid_routes() {
        let result = DRUPAL_RESOLVER.extract("some.routing.yml", "# empty\n");
        assert_eq!(result.nodes.len(), 0);
        assert_eq!(result.references.len(), 0);
    }
}

// ---------------------------------------------------------------------------
// extract() - hook detection in .module files
// ---------------------------------------------------------------------------

// describe('drupalResolver.extract - hook detection')
mod drupal_resolver_extract_hook_detection {
    use super::*;

    // it('detects hook implementation via docblock (Strategy A)')
    #[test]
    fn detects_hook_implementation_via_docblock_strategy_a() {
        let src = r#"<?php

/**
 * Implements hook_form_alter().
 */
function mymodule_form_alter(&$form, $form_state, $form_id) {
  // ...
}
"#;
        let result = DRUPAL_RESOLVER.extract("web/modules/custom/mymodule/mymodule.module", src);
        let hook_ref = result
            .references
            .iter()
            .find(|reference| reference.reference_name == "hook_form_alter");
        assert!(hook_ref.is_some());
        assert_eq!(hook_ref.unwrap().reference_kind, ReferenceKind::References);
    }

    // it('detects hook implementation via name pattern (Strategy B)')
    #[test]
    fn detects_hook_implementation_via_name_pattern_strategy_b() {
        let src = r#"<?php

function mymodule_views_data() {
  return [];
}
"#;
        let result = DRUPAL_RESOLVER.extract("web/modules/custom/mymodule/mymodule.module", src);
        let hook_ref = result
            .references
            .iter()
            .find(|reference| reference.reference_name == "hook_views_data");
        assert!(hook_ref.is_some());
    }

    // it('does not emit a hook ref for non-hook helper functions')
    #[test]
    fn does_not_emit_a_hook_ref_for_non_hook_helper_functions() {
        // 'other_module_helper' doesn't start with 'mymodule_', so no hook ref.
        let src = r#"<?php
function other_module_helper() {}
"#;
        let result = DRUPAL_RESOLVER.extract("web/modules/custom/mymodule/mymodule.module", src);
        assert_eq!(result.references.len(), 0);
    }

    // it('detects hooks in .install files')
    #[test]
    fn detects_hooks_in_install_files() {
        let src = r#"<?php
/**
 * Implements hook_schema().
 */
function mymodule_schema() {
  return [];
}
"#;
        let result = DRUPAL_RESOLVER.extract("web/modules/custom/mymodule/mymodule.install", src);
        let hook_ref = result
            .references
            .iter()
            .find(|reference| reference.reference_name == "hook_schema");
        assert!(hook_ref.is_some());
    }

    // it('detects hooks in .theme files')
    #[test]
    fn detects_hooks_in_theme_files() {
        let src = r#"<?php
/**
 * Implements hook_preprocess_node().
 */
function mytheme_preprocess_node(&$variables) {}
"#;
        let result = DRUPAL_RESOLVER.extract("web/themes/custom/mytheme/mytheme.theme", src);
        let hook_ref = result
            .references
            .iter()
            .find(|reference| reference.reference_name == "hook_preprocess_node");
        assert!(hook_ref.is_some());
    }

    // it('does not duplicate refs when both docblock and name pattern match')
    #[test]
    fn does_not_duplicate_refs_when_both_docblock_and_name_pattern_match() {
        // Strategy A matches first and adds to docblockMatched set;
        // Strategy B skips already-matched functions.
        let src = r#"<?php
/**
 * Implements hook_form_alter().
 */
function mymodule_form_alter(&$form, $form_state, $form_id) {}
"#;
        let result = DRUPAL_RESOLVER.extract("web/modules/custom/mymodule/mymodule.module", src);
        let hook_refs = result
            .references
            .iter()
            .filter(|reference| reference.reference_name == "hook_form_alter")
            .collect::<Vec<_>>();
        assert_eq!(hook_refs.len(), 1);
    }
}

// ---------------------------------------------------------------------------
// resolve()
// ---------------------------------------------------------------------------

// describe('drupalResolver.resolve')
mod drupal_resolver_resolve {
    use super::*;

    // it('resolves a _controller FQCN with ::method to the method node')
    #[test]
    fn resolves_a_controller_fqcn_with_method_to_the_method_node() {
        let method_node = node(
            "method:abc123",
            NodeKind::Method,
            "build",
            "MyController::build",
            "web/modules/custom/mymodule/src/Controller/MyController.php",
            Language::Php,
            10,
            20,
        );
        let class_node = node(
            "class:def456",
            NodeKind::Class,
            "MyController",
            "MyController",
            "web/modules/custom/mymodule/src/Controller/MyController.php",
            Language::Php,
            5,
            30,
        );
        let mut ctx = make_context().with_nodes(vec![class_node, method_node]);
        let ref_ = unresolved_ref(
            "\\Drupal\\mymodule\\Controller\\MyController::build",
            "mymodule.routing.yml",
            Language::Yaml,
        );

        let resolved = DRUPAL_RESOLVER.resolve(&ref_, &mut ctx);
        assert!(resolved.is_some());
        let resolved = resolved.unwrap();
        assert_eq!(resolved.target_node_id, "method:abc123");
        assert!(resolved.confidence >= 0.85);
    }

    // it('resolves a _form FQCN (no ::method) to the class node')
    #[test]
    fn resolves_a_form_fqcn_no_method_to_the_class_node() {
        let class_node = node(
            "class:form123",
            NodeKind::Class,
            "SettingsForm",
            "SettingsForm",
            "web/modules/custom/mymodule/src/Form/SettingsForm.php",
            Language::Php,
            1,
            50,
        );
        let mut ctx = make_context().with_nodes(vec![class_node]);
        let ref_ = unresolved_ref(
            "\\Drupal\\mymodule\\Form\\SettingsForm",
            "mymodule.routing.yml",
            Language::Yaml,
        );

        let resolved = DRUPAL_RESOLVER.resolve(&ref_, &mut ctx);
        assert!(resolved.is_some());
        assert_eq!(resolved.unwrap().target_node_id, "class:form123");
    }

    // it('returns null when the target class cannot be found')
    #[test]
    fn returns_null_when_the_target_class_cannot_be_found() {
        let mut ctx = make_context();
        let ref_ = unresolved_ref(
            "\\Drupal\\mymodule\\Controller\\Missing::method",
            "mymodule.routing.yml",
            Language::Yaml,
        );
        assert!(DRUPAL_RESOLVER.resolve(&ref_, &mut ctx).is_none());
    }

    // it('resolves a single-colon controller-service ref (Class:method)')
    #[test]
    fn resolves_a_single_colon_controller_service_ref_class_method() {
        let method_node = node(
            "method:nojs1",
            NodeKind::Method,
            "setNoJsCookie",
            "BigPipeController::setNoJsCookie",
            "core/modules/big_pipe/src/Controller/BigPipeController.php",
            Language::Php,
            10,
            20,
        );
        let class_node = node(
            "class:nojs2",
            NodeKind::Class,
            "BigPipeController",
            "BigPipeController",
            "core/modules/big_pipe/src/Controller/BigPipeController.php",
            Language::Php,
            5,
            30,
        );
        let mut ctx = make_context().with_nodes(vec![class_node, method_node]);
        let ref_ = unresolved_ref(
            "\\Drupal\\big_pipe\\Controller\\BigPipeController:setNoJsCookie",
            "big_pipe.routing.yml",
            Language::Yaml,
        );

        let resolved = DRUPAL_RESOLVER.resolve(&ref_, &mut ctx);
        assert!(resolved.is_some());
        assert_eq!(resolved.unwrap().target_node_id, "method:nojs1");
    }
}

// ---------------------------------------------------------------------------
// End-to-end integration test
// ---------------------------------------------------------------------------

// describe('Drupal end-to-end - route node linked to controller method')
mod drupal_end_to_end_route_node_linked_to_controller_method {
    use super::*;

    // it('creates a route->controller edge from routing.yml to PHP class')
    #[test]
    fn creates_a_route_controller_edge_from_routing_yml_to_php_class() {
        before_all_init_grammars();
        let project = TempProject::new("cg-drupal");

        // Minimal composer.json to trigger Drupal detection.
        project.write(
            "composer.json",
            r#"{"require":{"drupal/core-recommended":"~10.5"}}"#,
        );

        // Module directory structure.
        project.mkdir("web/modules/custom/my_module/src/Controller");

        // routing.yml
        project.write(
            "web/modules/custom/my_module/my_module.routing.yml",
            &([
                "my_module.hello:",
                "  path: '/hello'",
                "  defaults:",
                "    _controller: '\\Drupal\\my_module\\Controller\\HelloController::build'",
                "    _title: 'Hello'",
                "  requirements:",
                "    _permission: 'access content'",
            ]
            .join("\n")
                + "\n"),
        );

        // PHP controller.
        project.write(
            "web/modules/custom/my_module/src/Controller/HelloController.php",
            &([
                "<?php",
                "namespace Drupal\\my_module\\Controller;",
                "use Drupal\\Core\\Controller\\ControllerBase;",
                "class HelloController extends ControllerBase {",
                "  public function build() {",
                "    return ['#markup' => 'Hello'];",
                "  }",
                "}",
            ]
            .join("\n")
                + "\n"),
        );

        let mut cg = CodeGraph::init_sync(project.path()).expect("CodeGraph should initialize");
        let result = cg.index_all(IndexOptions::default());
        assert!(result.success, "indexing failed: {:?}", result.errors);

        // Route node must exist.
        let routes = cg.get_nodes_by_kind(NodeKind::Route);
        assert!(!routes.is_empty());
        let route = routes
            .iter()
            .find(|node| node.name.contains("/hello"))
            .expect("route /hello should be defined");

        // Controller method must be indexed.
        let methods = cg.get_nodes_by_kind(NodeKind::Method);
        let build_method = methods.iter().find(|node| node.name == "build");
        assert!(build_method.is_some());

        // Edge: route -> build method (or class fallback).
        let edges = cg.get_outgoing_edges(&route.id);
        assert!(!edges.is_empty());

        cg.close();
    }
}
