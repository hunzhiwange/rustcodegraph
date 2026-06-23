//! Dynamic-boundary surfacing (#687).
//!
//! This is the Rust port of `__tests__/dynamic-boundaries.test.ts`.
//! Scanner unit coverage runs against the translated Rust scanner, and the
//! `rustcodegraph_explore` integration parity cases now exercise the Rust MCP
//! facade end to end.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use regex::Regex;
use rustcodegraph::mcp::dynamic_boundaries::scan_dynamic_dispatch;
use rustcodegraph::mcp::tools::{ToolHandler, ToolResult};
use rustcodegraph::{CodeGraph, IndexOptions};
use serde_json::{Map, Value, json};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn lines(parts: &[&str]) -> String {
    parts.join("\n")
}

mod scan_dynamic_dispatch {
    use super::*;

    #[test]
    fn detects_a_computed_member_call_with_a_literal_key() {
        let body = "function go(p) {\n  table['save'](p);\n}";
        let m = scan_dynamic_dispatch(body, "typescript", 10);

        assert_eq!(m.len(), 1);
        assert_eq!(m[0].form, "computed-call");
        assert_eq!(m[0].key.as_deref(), Some("save"));
        assert_eq!(m[0].line, 11);
        assert!(m[0].snippet.contains("table['save'](p)"));
    }

    #[test]
    fn detects_a_computed_member_call_with_a_runtime_key_no_key_extracted() {
        let body = "dispatch(action) {\n  this.handlers[action.type](action.payload);\n}";
        let m = scan_dynamic_dispatch(body, "typescript", 1);

        assert_eq!(m.len(), 1);
        assert_eq!(m[0].form, "computed-call");
        assert_eq!(m[0].key, None);
    }

    #[test]
    fn does_not_fire_on_dispatch_shapes_inside_comments_or_strings() {
        let body = lines(&[
            "function safe() {",
            "  // this.handlers[action.type](payload) - commented out",
            "  const doc = \"call handlers[key](p) to dispatch\";",
            "  return 1;",
            "}",
        ]);

        assert_eq!(scan_dynamic_dispatch(&body, "typescript", 1).len(), 0);
    }

    #[test]
    fn does_not_treat_plain_indexing_or_array_literals_as_dispatch() {
        let body =
            "function f(xs) {\n  const a = xs[0];\n  const b = [1, 2, 3];\n  return a + b[1];\n}";

        assert_eq!(scan_dynamic_dispatch(body, "typescript", 1).len(), 0);
    }

    #[test]
    fn detects_python_getattr_immediate_call() {
        let body = "def run(self, name):\n    return getattr(self, name)(1)";
        let m = scan_dynamic_dispatch(body, "python", 5);

        assert_eq!(m.len(), 1);
        assert_eq!(m[0].form, "getattr-call");
    }

    #[test]
    fn detects_two_step_getattr_only_when_the_assigned_name_is_called_later() {
        let called = "def process(self, kind, p):\n    handler = getattr(self, 'handle_' + kind)\n    return handler(p)";
        let m = scan_dynamic_dispatch(called, "python", 1);

        assert_eq!(m.len(), 1);
        assert_eq!(m[0].form, "getattr-assign");
        assert_eq!(m[0].key.as_deref(), Some("handle_"));

        let not_called = "def peek(self, kind):\n    handler = getattr(self, 'handle_' + kind)\n    return handler";
        assert_eq!(scan_dynamic_dispatch(not_called, "python", 1).len(), 0);
    }

    #[test]
    fn detects_ruby_send_with_a_symbol_key() {
        let body = "def run(name)\n  target.send(:handle_save, 1)\nend";
        let m = scan_dynamic_dispatch(body, "ruby", 1);

        assert_eq!(m.len(), 1);
        assert_eq!(m[0].form, "ruby-send");
        assert_eq!(m[0].key.as_deref(), Some("handle_save"));
    }

    #[test]
    fn detects_typed_message_dispatch_and_marks_the_key_as_a_type() {
        let body = "public async Task<int> Create(CreateCmd c) {\n  return await _mediator.Send(new CreateTodoItemCommand(c));\n}";
        let m = scan_dynamic_dispatch(body, "csharp", 1);

        assert_eq!(m.len(), 1);
        assert_eq!(m[0].form, "typed-bus");
        assert_eq!(m[0].key.as_deref(), Some("CreateTodoItemCommand"));
        assert_eq!(m[0].key_is_type, Some(true));
    }

    #[test]
    fn detects_runtime_keyed_emit_but_not_literal_keyed_emit() {
        let runtime = "notify(name, data) {\n  this.emitter.emit(name, data);\n}";
        let m = scan_dynamic_dispatch(runtime, "typescript", 1);

        assert_eq!(m.len(), 1);
        assert_eq!(m[0].form, "var-key-dispatch");

        // Literal keys are the edge synthesizer's territory - not a boundary.
        let literal = "notify(data) {\n  this.emitter.emit('saved', data);\n}";
        assert_eq!(scan_dynamic_dispatch(literal, "typescript", 1).len(), 0);
    }

    #[test]
    fn dedupes_repeated_same_form_same_key_sites_and_counts_the_extras() {
        let body = lines(&[
            "route(a) {",
            "  this.table[a.type](a.p);",
            "  this.table[a.kind](a.p);",
            "  this.table[a.name](a.p);",
            "}",
        ]);
        let m = scan_dynamic_dispatch(&body, "typescript", 1);

        assert_eq!(m.len(), 1);
        assert_eq!(m[0].more_sites, Some(2));
    }

    #[test]
    fn detects_reflective_dispatch_with_a_literal_method_name_as_key() {
        let body =
            "public void run(Object o) {\n  o.getClass().getMethod(\"handlePing\").invoke(o);\n}";
        let m = scan_dynamic_dispatch(body, "java", 1);

        assert!(!m.is_empty());
        assert_eq!(m[0].form, "reflection");
        assert_eq!(m[0].key.as_deref(), Some("handlePing"));
    }
}

struct TempProject {
    root: PathBuf,
}

impl TempProject {
    fn new() -> Self {
        let base = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after Unix epoch")
            .as_nanos();
        for _ in 0..100 {
            let seq = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
            let root = std::env::temp_dir().join(format!(
                "codegraph-boundary-{}-{base}-{seq}",
                std::process::id()
            ));
            match fs::create_dir(&root) {
                Ok(()) => {
                    fs::create_dir(root.join("src")).unwrap_or_else(|err| {
                        panic!(
                            "failed to create fixture src directory {}: {err}",
                            root.display()
                        )
                    });
                    return Self { root };
                }
                Err(err) if err.kind() == ErrorKind::AlreadyExists => continue,
                Err(err) => panic!("failed to create fixture root {}: {err}", root.display()),
            }
        }
        panic!("failed to allocate unique dynamic-boundary fixture root");
    }

    fn path(&self) -> &Path {
        &self.root
    }

    fn write_src(&self, name: &str, content: &str) {
        let path = self.root.join("src").join(name);
        fs::write(&path, content)
            .unwrap_or_else(|err| panic!("failed to write fixture {}: {err}", path.display()));
    }
}

impl Drop for TempProject {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

struct Fixture {
    _temp: TempProject,
    cg: CodeGraph,
    handler: ToolHandler,
}

impl Fixture {
    fn setup(files: &[(&str, String)], _include: &[&str]) -> Self {
        let temp = TempProject::new();
        for (name, content) in files {
            temp.write_src(name, content);
        }

        let mut cg = CodeGraph::init_sync(temp.path()).expect("failed to initialize CodeGraph");
        let result = cg.index_all(IndexOptions::default());
        assert!(
            result.success,
            "index_all should succeed, errors: {:?}",
            result.errors
        );
        let mut handler = ToolHandler::new(true);
        handler.set_default_code_graph(&cg);

        Self {
            _temp: temp,
            cg,
            handler,
        }
    }

    fn explore(&mut self, query: &str) -> String {
        let result = self
            .handler
            .execute("rustcodegraph_explore", &query_args(query));
        first_text(&result).to_string()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.handler.close_all();
        self.cg.destroy();
    }
}

fn query_args(query: &str) -> Map<String, Value> {
    let mut args = Map::new();
    args.insert("query".to_string(), json!(query));
    args
}

fn first_text(result: &ToolResult) -> &str {
    result
        .content
        .first()
        .map(|content| content.text.as_str())
        .unwrap_or("")
}

mod codegraph_explore_dynamic_boundaries {
    use super::*;

    #[test]
    fn announces_the_boundary_site_and_shortlists_the_keyed_candidate() {
        let mut fixture = Fixture::setup(
            &[
                (
                    "router.ts",
                    lines(&[
                        "type Handler = (p: unknown) => void;",
                        "export class Router {",
                        "  private table: Record<string, Handler> = {};",
                        "  add(key: string, fn: Handler) { this.table[key] = fn; }",
                        "  routeSave(payload: unknown) {",
                        "    this.table['save'](payload);",
                        "  }",
                        "}",
                    ]),
                ),
                (
                    "handlers.ts",
                    lines(&[
                        "import { Router } from './router';",
                        "export function onSave(payload: unknown) { return payload; }",
                        "export function wire(r: Router) { r.add(\"save\", onSave); }",
                    ]),
                ),
            ],
            &["**/*.ts"],
        );

        let text = fixture.explore("routeSave onSave");

        assert!(text.contains("## Dynamic boundaries"));
        assert!(text.contains("computed member call"));
        assert!(Regex::new(r"router\.ts:6").unwrap().is_match(&text));
        assert!(text.contains("candidates for key `save`"));
        assert!(text.contains("onSave"));
        assert!(text.contains("← you named this"));
        assert!(!Regex::new(r"(?i)\buse Read\b").unwrap().is_match(&text));
    }

    #[test]
    fn announces_a_runtime_keyed_boundary_with_no_candidate_list() {
        let mut fixture = Fixture::setup(
            &[
                (
                    "bus.ts",
                    lines(&[
                        "type Action = { type: string; payload?: unknown };",
                        "type Handler = (p: unknown) => void;",
                        "export class Bus {",
                        "  private table: Record<string, Handler> = {};",
                        "  route(action: Action) {",
                        "    this.table[action.type](action.payload);",
                        "  }",
                        "}",
                    ]),
                ),
                (
                    "handlers.ts",
                    "export function onSave(payload: unknown) { return payload; }".to_string(),
                ),
            ],
            &["**/*.ts"],
        );

        let text = fixture.explore("route onSave");

        assert!(text.contains("## Dynamic boundaries"));
        assert!(text.contains("computed member call"));
        assert!(!text.contains("candidates for key"));
    }

    #[test]
    fn surfaces_the_boundary_even_when_the_other_symbol_is_not_in_the_graph() {
        let mut fixture = Fixture::setup(
            &[(
                "bus.ts",
                lines(&[
                    "type Action = { type: string; payload?: unknown };",
                    "type Handler = (p: unknown) => void;",
                    "export class Bus {",
                    "  private table: Record<string, Handler> = {};",
                    "  route(action: Action) {",
                    "    this.table[action.type](action.payload);",
                    "  }",
                    "}",
                ]),
            )],
            &["**/*.ts"],
        );

        let text = fixture.explore("route processPayment");
        assert!(text.contains("## Dynamic boundaries"));
    }

    #[test]
    fn renders_a_direct_synthesized_emit_handler_hop_as_a_dynamic_dispatch_link_687_criterion_1() {
        let mut fixture = Fixture::setup(
            &[
                (
                    "bus.ts",
                    lines(&[
                        "type Handler = (p: unknown) => void;",
                        "export class EventBus {",
                        "  private listeners: Record<string, Handler[]> = {};",
                        "  on(event: string, fn: Handler) { (this.listeners[event] ??= []).push(fn); }",
                        "  emit(event: string, payload: unknown) { for (const fn of this.listeners[event] ?? []) fn(payload); }",
                        "}",
                        "export const bus = new EventBus();",
                    ]),
                ),
                (
                    "billing.ts",
                    lines(&[
                        "import { bus } from './bus';",
                        "export function settleInvoice(payload: unknown) { return payload; }",
                        "bus.on('invoice.settled', settleInvoice);",
                    ]),
                ),
                (
                    "checkout.ts",
                    lines(&[
                        "import { bus } from './bus';",
                        "export function completeCheckout(order: unknown) {",
                        "  bus.emit('invoice.settled', order);",
                        "}",
                    ]),
                ),
            ],
            &["**/*.ts"],
        );

        let text = fixture.explore("completeCheckout settleInvoice");

        assert!(text.contains("## Dynamic-dispatch links among your symbols"));
        assert!(
            Regex::new("completeCheckout → settleInvoice")
                .unwrap()
                .is_match(&text)
        );
        assert!(text.contains("invoice.settled"));
        assert!(!text.contains("## Dynamic boundaries"));
    }

    #[test]
    fn never_adds_the_section_to_a_fully_connected_flow() {
        let mut fixture = Fixture::setup(
            &[(
                "pipeline.ts",
                lines(&[
                    "export function stepOne() { return stepTwo(); }",
                    "export function stepTwo() { return stepThree(); }",
                    "export function stepThree() { return 3; }",
                ]),
            )],
            &["**/*.ts"],
        );

        let text = fixture.explore("stepOne stepThree");
        assert!(text.contains("## Flow"));
        assert!(!text.contains("## Dynamic boundaries"));
    }

    #[test]
    fn python_getattr_dispatch_surfaces_with_a_prefix_key_candidate() {
        let mut fixture = Fixture::setup(
            &[(
                "service.py",
                lines(&[
                    "class Service:",
                    "    def handle_save(self, payload):",
                    "        return payload",
                    "",
                    "    def process(self, kind, payload):",
                    "        handler = getattr(self, 'handle_' + kind)",
                    "        return handler(payload)",
                ]),
            )],
            &["**/*.py"],
        );

        let text = fixture.explore("process handle_save");

        assert!(text.contains("## Dynamic boundaries"));
        assert!(text.contains("getattr"));
        assert!(text.contains("handle_save"));
    }
}
