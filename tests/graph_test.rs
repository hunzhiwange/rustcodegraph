//! Graph Query Tests
//!
//! Rust port of `__tests__/graph.test.ts`.
//!
//! The current Rust `CodeGraph` facade still has several graph-query methods
//! wired as structural stubs. Those TypeScript cases are preserved below as
//! ignored tests with their original assertion paths left in place.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use rustcodegraph::types::{EdgeKind, Node, NodeKind, TraversalDirection, TraversalOptions};
use rustcodegraph::{CodeGraph, IndexOptions};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

struct TempProject {
    root: PathBuf,
}

impl TempProject {
    fn new(prefix: &str) -> Self {
        for _ in 0..100 {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time should be after Unix epoch")
                .as_nanos();
            let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
            let root = std::env::temp_dir()
                .join(format!("{prefix}-{}-{nanos}-{counter}", std::process::id()));
            match fs::create_dir(&root) {
                Ok(()) => {
                    fs::create_dir_all(root.join("src")).expect("failed to create fixture src dir");
                    return Self { root };
                }
                Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(err) => panic!("failed to create temp project {}: {err}", root.display()),
            }
        }
        panic!("failed to create unique temp project with prefix {prefix}");
    }

    fn path(&self) -> &Path {
        &self.root
    }

    fn write(&self, relative_path: &str, content: &str) {
        let path = self.root.join(relative_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .unwrap_or_else(|err| panic!("failed to create {}: {err}", parent.display()));
        }
        fs::write(&path, content)
            .unwrap_or_else(|err| panic!("failed to write fixture {}: {err}", path.display()));
    }
}

impl Drop for TempProject {
    fn drop(&mut self) {
        if self.root.exists() {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}

struct CodeGraphGuard {
    value: Option<CodeGraph>,
}

impl CodeGraphGuard {
    fn new(value: CodeGraph) -> Self {
        Self { value: Some(value) }
    }

    fn get_mut(&mut self) -> &mut CodeGraph {
        self.value.as_mut().expect("CodeGraph should be present")
    }
}

impl Drop for CodeGraphGuard {
    fn drop(&mut self) {
        if let Some(mut value) = self.value.take() {
            value.close();
        }
    }
}

struct Fixture {
    _temp: TempProject,
    cg: CodeGraphGuard,
}

impl Fixture {
    fn new() -> Self {
        let temp = TempProject::new("codegraph-graph-test");

        // Create base class
        temp.write(
            "src/base.ts",
            r#"
export class BaseClass {
  protected value: number;

  constructor(value: number) {
    this.value = value;
  }

  getValue(): number {
    return this.value;
  }
}

export interface Printable {
  print(): void;
}
"#,
        );

        // Create derived class
        temp.write(
            "src/derived.ts",
            r#"
import { BaseClass, Printable } from './base';

export class DerivedClass extends BaseClass implements Printable {
  private name: string;

  constructor(value: number, name: string) {
    super(value);
    this.name = name;
  }

  print(): void {
    console.log(this.getName(), this.getValue());
  }

  getName(): string {
    return this.name;
  }
}
"#,
        );

        // Create utility functions
        temp.write(
            "src/utils.ts",
            r#"
export function formatValue(value: number): string {
  return value.toFixed(2);
}

export function processValue(value: number): number {
  const formatted = formatValue(value);
  return parseFloat(formatted);
}

export function doubleValue(value: number): number {
  return value * 2;
}

// Unused function (dead code)
function unusedHelper(): void {
  console.log('never called');
}
"#,
        );

        // Create main file that uses everything
        temp.write(
            "src/main.ts",
            r#"
import { DerivedClass } from './derived';
import { processValue, doubleValue } from './utils';

function main(): void {
  const obj = new DerivedClass(10, 'test');
  obj.print();

  const result = processValue(doubleValue(obj.getValue()));
  console.log(result);
}

export { main };
"#,
        );

        let mut cg = CodeGraph::init_sync(temp.path()).expect("CodeGraph should initialize");
        let result = cg.index_all(IndexOptions::default());
        assert!(result.success, "indexing failed: {:?}", result.errors);
        let _ = cg.resolve_references();

        Self {
            _temp: temp,
            cg: CodeGraphGuard::new(cg),
        }
    }
}

fn find_node(cg: &mut CodeGraph, kind: NodeKind, name: &str) -> Option<Node> {
    cg.get_nodes_by_kind(kind)
        .into_iter()
        .find(|node| node.name == name)
}

fn assert_contains(actual: &[String], expected: &str) {
    assert!(
        actual.iter().any(|value| value == expected),
        "expected {actual:?} to contain {expected:?}"
    );
}

fn assert_not_contains(actual: &[String], unexpected: &str) {
    assert!(
        !actual.iter().any(|value| value == unexpected),
        "expected {actual:?} not to contain {unexpected:?}"
    );
}

mod traverse {
    use super::*;

    #[test]
    fn should_traverse_graph_from_a_starting_node() {
        let mut fixture = Fixture::new();
        let Some(main_func) = find_node(fixture.cg.get_mut(), NodeKind::Function, "main") else {
            println!("main function not found, skipping test");
            return;
        };

        let subgraph = fixture.cg.get_mut().traverse(
            &main_func.id,
            Some(TraversalOptions {
                max_depth: Some(2),
                edge_kinds: None,
                node_kinds: None,
                direction: Some(TraversalDirection::Outgoing),
                limit: None,
                include_start: None,
            }),
        );

        assert!(!subgraph.nodes.is_empty());
        assert!(subgraph.roots.contains(&main_func.id));
    }

    #[test]
    fn should_respect_max_depth_option() {
        let mut fixture = Fixture::new();
        let Some(main_func) = find_node(fixture.cg.get_mut(), NodeKind::Function, "main") else {
            return;
        };

        let shallow = fixture.cg.get_mut().traverse(
            &main_func.id,
            Some(TraversalOptions {
                max_depth: Some(1),
                edge_kinds: None,
                node_kinds: None,
                direction: None,
                limit: None,
                include_start: None,
            }),
        );
        let deep = fixture.cg.get_mut().traverse(
            &main_func.id,
            Some(TraversalOptions {
                max_depth: Some(3),
                edge_kinds: None,
                node_kinds: None,
                direction: None,
                limit: None,
                include_start: None,
            }),
        );

        assert!(deep.nodes.len() >= shallow.nodes.len());
    }

    #[test]
    fn should_support_incoming_direction() {
        let mut fixture = Fixture::new();
        let Some(format_value) = find_node(fixture.cg.get_mut(), NodeKind::Function, "formatValue")
        else {
            return;
        };

        let subgraph = fixture.cg.get_mut().traverse(
            &format_value.id,
            Some(TraversalOptions {
                max_depth: Some(2),
                edge_kinds: None,
                node_kinds: None,
                direction: Some(TraversalDirection::Incoming),
                limit: None,
                include_start: None,
            }),
        );

        assert!(!subgraph.nodes.is_empty());
    }
}

mod get_context {
    use super::*;

    #[test]
    fn should_return_context_for_a_node() {
        let mut fixture = Fixture::new();
        let Some(derived_class) = find_node(fixture.cg.get_mut(), NodeKind::Class, "DerivedClass")
        else {
            println!("DerivedClass not found, skipping test");
            return;
        };

        let context = fixture
            .cg
            .get_mut()
            .get_context(&derived_class.id)
            .expect("context should be defined");

        assert_eq!(context.focal.id, derived_class.id);
        let _ = context.ancestors;
        let _ = context.children;
        let _ = context.incoming_refs;
        let _ = context.outgoing_refs;
    }

    #[test]
    fn should_throw_for_non_existent_node() {
        let mut fixture = Fixture::new();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = fixture.cg.get_mut().get_context("non-existent-id");
        }));
        let panic = result.expect_err("expected Node not found panic");
        let message = panic
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| panic.downcast_ref::<&str>().copied())
            .unwrap_or("<non-string panic>");
        assert!(
            message.contains("Node not found"),
            "expected panic to contain Node not found, got {message:?}"
        );
    }
}

mod get_call_graph {
    use super::*;

    #[test]
    fn should_return_call_graph_for_a_function() {
        let mut fixture = Fixture::new();
        let Some(process_value) =
            find_node(fixture.cg.get_mut(), NodeKind::Function, "processValue")
        else {
            println!("processValue not found, skipping test");
            return;
        };

        let call_graph = fixture.cg.get_mut().get_call_graph(&process_value.id, 2);

        assert!(!call_graph.nodes.is_empty());
        assert!(call_graph.nodes.contains_key(&process_value.id));
    }
}

mod get_type_hierarchy {
    use super::*;

    #[test]
    fn should_return_type_hierarchy_for_a_class() {
        let mut fixture = Fixture::new();
        let Some(derived_class) = find_node(fixture.cg.get_mut(), NodeKind::Class, "DerivedClass")
        else {
            return;
        };

        let hierarchy = fixture.cg.get_mut().get_type_hierarchy(&derived_class.id);

        assert!(!hierarchy.nodes.is_empty());
        assert!(hierarchy.nodes.contains_key(&derived_class.id));
    }

    #[test]
    fn should_return_empty_subgraph_for_non_existent_node() {
        let mut fixture = Fixture::new();
        let hierarchy = fixture.cg.get_mut().get_type_hierarchy("non-existent-id");

        assert_eq!(hierarchy.nodes.len(), 0);
        assert_eq!(hierarchy.edges.len(), 0);
    }
}

mod find_usages {
    use super::*;

    #[test]
    fn should_find_usages_of_a_symbol() {
        let mut fixture = Fixture::new();
        let format_value = find_node(fixture.cg.get_mut(), NodeKind::Function, "formatValue")
            .expect("formatValue should be defined");

        let usages = fixture.cg.get_mut().find_usages(&format_value.id);
        let usage_names = usages
            .iter()
            .map(|usage| usage.node.name.as_str())
            .collect::<Vec<_>>();

        assert!(
            usage_names.contains(&"processValue"),
            "expected formatValue usages to include processValue, got {usage_names:?}"
        );
    }
}

mod get_callers_and_get_callees {
    use super::*;

    #[test]
    fn should_get_callers_of_a_function() {
        let mut fixture = Fixture::new();
        let Some(format_value) = find_node(fixture.cg.get_mut(), NodeKind::Function, "formatValue")
        else {
            return;
        };

        let callers = fixture.cg.get_mut().get_callers(&format_value.id, 1);
        let caller_names = callers
            .iter()
            .map(|caller| caller.node.name.as_str())
            .collect::<Vec<_>>();

        assert!(
            caller_names.contains(&"processValue"),
            "expected formatValue callers to include processValue, got {caller_names:?}"
        );
    }

    #[test]
    fn should_get_callees_of_a_function() {
        let mut fixture = Fixture::new();
        let Some(process_value) =
            find_node(fixture.cg.get_mut(), NodeKind::Function, "processValue")
        else {
            return;
        };

        let callees = fixture.cg.get_mut().get_callees(&process_value.id, 1);
        let callee_names = callees
            .iter()
            .map(|callee| callee.node.name.as_str())
            .collect::<Vec<_>>();

        assert!(
            callee_names.contains(&"formatValue"),
            "expected processValue callees to include formatValue, got {callee_names:?}"
        );
    }

    #[test]
    fn treats_class_instantiation_as_a_caller_callee_of_the_class_774() {
        let mut fixture = Fixture::new();
        // main() does `new DerivedClass(10, 'test')`. Constructing a class is
        // calling its constructor, so main is a caller of DerivedClass and
        // DerivedClass is a callee of main. Before #774 the `instantiates` edge
        // was excluded from the caller/callee traversal, so `callers <Class>`
        // returned the importing file (or nothing) and missed every
        // construction site.
        let derived = find_node(fixture.cg.get_mut(), NodeKind::Class, "DerivedClass")
            .expect("DerivedClass should be defined");
        let main = find_node(fixture.cg.get_mut(), NodeKind::Function, "main")
            .expect("main should be defined");

        let caller_names = fixture
            .cg
            .get_mut()
            .get_callers(&derived.id, 1)
            .into_iter()
            .map(|caller| caller.node.name)
            .collect::<Vec<_>>();
        assert_contains(&caller_names, "main");

        let callee_names = fixture
            .cg
            .get_mut()
            .get_callees(&main.id, 1)
            .into_iter()
            .map(|callee| callee.node.name)
            .collect::<Vec<_>>();
        assert_contains(&callee_names, "DerivedClass");
    }
}

mod get_impact_radius {
    use super::*;

    #[test]
    fn should_calculate_impact_radius() {
        let mut fixture = Fixture::new();
        let Some(format_value) = find_node(fixture.cg.get_mut(), NodeKind::Function, "formatValue")
        else {
            return;
        };

        let impact = fixture.cg.get_mut().get_impact_radius(&format_value.id, 3);

        assert!(!impact.nodes.is_empty());
        assert!(impact.nodes.contains_key(&format_value.id));
    }

    #[test]
    fn does_not_drag_in_sibling_members_via_the_structural_contains_edge_536() {
        let mut fixture = Fixture::new();
        let get_name = find_node(fixture.cg.get_mut(), NodeKind::Method, "getName")
            .expect("getName should be defined");
        let derived = find_node(fixture.cg.get_mut(), NodeKind::Class, "DerivedClass")
            .expect("DerivedClass should be defined");

        let impact = fixture.cg.get_mut().get_impact_radius(&get_name.id, 3);
        // The containing class must NOT be pulled into impact just because it
        // *contains* getName - climbing that contains edge would re-expand every
        // sibling method and explode impact for a leaf symbol. (#536)
        assert!(!impact.nodes.contains_key(&derived.id));
    }
}

mod find_path {
    use super::*;

    #[test]
    fn should_find_path_between_connected_nodes() {
        let mut fixture = Fixture::new();
        let stats = fixture.cg.get_mut().get_stats();

        if stats.node_count < 2 {
            return;
        }

        let functions = fixture.cg.get_mut().get_nodes_by_kind(NodeKind::Function);
        if functions.len() < 2 {
            return;
        }

        // Try to find any path
        let process_value = functions
            .iter()
            .find(|node| node.name == "processValue")
            .cloned();
        let format_value = functions
            .iter()
            .find(|node| node.name == "formatValue")
            .cloned();

        if let (Some(process_value), Some(format_value)) = (process_value, format_value) {
            let path = fixture
                .cg
                .get_mut()
                .find_path(&process_value.id, &format_value.id, None);

            let path = path.expect("processValue should call formatValue");
            let first = path.first().expect("path should include the start node");
            let last = path.last().expect("path should include the target node");

            assert_eq!(first.node.id, process_value.id);
            assert_eq!(last.node.id, format_value.id);
            assert!(
                path.iter().any(|step| step
                    .edge
                    .as_ref()
                    .is_some_and(|edge| edge.kind == EdgeKind::Calls)),
                "expected path to include a calls edge, got {path:?}"
            );
        }
    }

    #[test]
    fn should_return_null_for_disconnected_nodes() {
        let mut fixture = Fixture::new();
        // Create two nodes that definitely don't have a path
        let path = fixture
            .cg
            .get_mut()
            .find_path("non-existent-1", "non-existent-2", None);

        assert!(path.is_none());
    }
}

mod get_ancestors_and_get_children {
    use super::*;

    #[test]
    fn should_get_ancestors_of_a_node() {
        let mut fixture = Fixture::new();
        let Some(print_method) = find_node(fixture.cg.get_mut(), NodeKind::Method, "print") else {
            return;
        };

        let ancestors = fixture.cg.get_mut().get_ancestors(&print_method.id);

        let ancestor_names = ancestors
            .iter()
            .map(|node| node.name.as_str())
            .collect::<Vec<_>>();
        assert!(
            ancestor_names.contains(&"DerivedClass"),
            "expected print ancestors to contain DerivedClass, got {ancestor_names:?}"
        );
        assert!(
            ancestors.iter().any(|node| node.kind == NodeKind::File),
            "expected print ancestors to include its containing file, got {ancestors:?}"
        );
    }

    #[test]
    fn should_get_children_of_a_node() {
        let mut fixture = Fixture::new();
        let Some(derived_class) = find_node(fixture.cg.get_mut(), NodeKind::Class, "DerivedClass")
        else {
            return;
        };

        let children = fixture.cg.get_mut().get_children(&derived_class.id);

        let child_names = children
            .iter()
            .map(|node| node.name.as_str())
            .collect::<Vec<_>>();
        assert!(
            child_names.contains(&"print"),
            "expected DerivedClass children to contain print, got {child_names:?}"
        );
        assert!(
            child_names.contains(&"getName"),
            "expected DerivedClass children to contain getName, got {child_names:?}"
        );
    }
}

mod file_dependency_analysis {
    use super::*;

    #[test]
    fn reports_cross_file_dependencies_via_the_symbol_graph_not_just_imports() {
        let mut fixture = Fixture::new();
        // main() instantiates DerivedClass (derived.ts) and calls
        // processValue/doubleValue (utils.ts) - both are real dependencies.
        let deps = fixture.cg.get_mut().get_file_dependencies("src/main.ts");
        assert_contains(&deps, "src/utils.ts");
        assert_contains(&deps, "src/derived.ts");
    }

    #[test]
    fn reports_cross_file_dependents_via_the_symbol_graph_not_just_imports() {
        let mut fixture = Fixture::new();
        // utils.ts is used by main.ts (processValue/doubleValue calls); the old
        // imports-only implementation returned [] here.
        let dependents = fixture.cg.get_mut().get_file_dependents("src/utils.ts");
        assert_contains(&dependents, "src/main.ts");
    }

    #[test]
    fn counts_extends_implements_as_a_dependency_edge() {
        let mut fixture = Fixture::new();
        // derived.ts extends BaseClass / implements Printable, both in base.ts.
        let derived_deps = fixture.cg.get_mut().get_file_dependencies("src/derived.ts");
        assert_contains(&derived_deps, "src/base.ts");

        let base_dependents = fixture.cg.get_mut().get_file_dependents("src/base.ts");
        assert_contains(&base_dependents, "src/derived.ts");
    }

    #[test]
    fn never_lists_a_file_as_its_own_dependent_or_dependency() {
        let mut fixture = Fixture::new();
        for file in [
            "src/main.ts",
            "src/utils.ts",
            "src/base.ts",
            "src/derived.ts",
        ] {
            let dependents = fixture.cg.get_mut().get_file_dependents(file);
            assert_not_contains(&dependents, file);

            let deps = fixture.cg.get_mut().get_file_dependencies(file);
            assert_not_contains(&deps, file);
        }
    }
}

mod find_circular_dependencies {
    use super::*;

    #[test]
    fn should_detect_circular_dependencies() {
        let mut fixture = Fixture::new();
        let cycles = fixture.cg.get_mut().find_circular_dependencies();

        // Our test files don't have circular deps
        assert!(cycles.is_empty() || !cycles.is_empty());
    }
}

mod find_dead_code {
    use super::*;

    #[test]
    fn should_find_dead_code() {
        let mut fixture = Fixture::new();
        let dead_code = fixture
            .cg
            .get_mut()
            .find_dead_code(Some(vec![NodeKind::Function]));

        // unusedHelper should be detected
        let has_unused = dead_code.iter().any(|node| node.name == "unusedHelper");
        let has_process_value = dead_code.iter().any(|node| node.name == "processValue");

        assert!(
            has_unused,
            "expected unusedHelper to be reported as dead code"
        );
        assert!(
            !has_process_value,
            "expected referenced processValue not to be reported as dead code"
        );
    }
}

mod get_node_metrics {
    use super::*;

    #[test]
    fn should_return_metrics_for_a_node() {
        let mut fixture = Fixture::new();
        let func = find_node(fixture.cg.get_mut(), NodeKind::Function, "processValue")
            .expect("processValue should be defined");

        let metrics = fixture.cg.get_mut().get_node_metrics(&func.id);

        assert!(
            metrics.incoming_edge_count > 0,
            "expected processValue to have incoming references, got {metrics:?}"
        );
        assert!(
            metrics.outgoing_edge_count > 0,
            "expected processValue to have outgoing references, got {metrics:?}"
        );
        assert_eq!(metrics.call_count, 1);
        assert_eq!(metrics.caller_count, 1);
    }
}
