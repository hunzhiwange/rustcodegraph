//! Function-as-value capture tests (#756).
//!
//! This is the Rust port of `__tests__/function-ref.test.ts`.
//!
//! The TypeScript suite exercises the full tree-sitter extraction and
//! reference-resolution pipeline. These Rust cases keep the facade backend at
//! parity for function-as-value extraction and resolution.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::Connection;
use rustcodegraph::directory::get_code_graph_dir;
use rustcodegraph::types::{Edge, EdgeKind, NodeKind};
use rustcodegraph::{CodeGraph, IndexOptions};

struct TempProject {
    root: PathBuf,
}

impl TempProject {
    fn new(prefix: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after Unix epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("{prefix}-{}-{nanos}", std::process::id()));
        fs::create_dir_all(&root).unwrap_or_else(|err| {
            panic!("failed to create temp project {}: {err}", root.display())
        });
        Self { root }
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
            .unwrap_or_else(|err| panic!("failed to write {}: {err}", path.display()));
    }

    fn write_lines(&self, relative_path: &str, lines: &[&str]) {
        self.write(relative_path, &lines.join("\n"));
    }
}

impl Drop for TempProject {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn index(project_root: &Path) -> CodeGraph {
    let mut cg = CodeGraph::init_sync(project_root).expect("failed to initialize CodeGraph");
    let result = cg.index_all(IndexOptions::default());
    assert!(
        result.success,
        "index_all should succeed, errors: {:?}",
        result.errors
    );
    cg
}

fn is_fn_ref(edge: &Edge) -> bool {
    edge.kind == EdgeKind::References
        && edge
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("fnRef"))
            .and_then(|value| value.as_bool())
            == Some(true)
}

/// Incoming edges to `name`'s node that came from function-as-value capture.
fn fn_ref_edges_into(cg: &mut CodeGraph, name: &str) -> Vec<Edge> {
    let targets = cg.get_nodes_by_name(name);
    let mut edges = Vec::new();
    for target in targets {
        for edge in cg.get_incoming_edges(&target.id) {
            if is_fn_ref(&edge) {
                edges.push(edge);
            }
        }
    }
    edges
}

/// Names of the source nodes of the given edges, sorted.
fn source_names(cg: &mut CodeGraph, edges: &[Edge]) -> Vec<String> {
    let mut names = Vec::new();
    for edge in edges {
        if let Some(node) = cg.get_node(&edge.source) {
            names.push(node.name);
        }
    }
    names.sort();
    names
}

fn assert_names_eq(actual: Vec<String>, expected: &[&str]) {
    assert_eq!(
        actual,
        expected
            .iter()
            .map(|name| (*name).to_owned())
            .collect::<Vec<_>>()
    );
}

fn assert_names_contains(actual: &[String], expected: &str) {
    assert!(
        actual.iter().any(|name| name == expected),
        "expected {actual:?} to contain {expected:?}"
    );
}

mod function_as_value_capture_756 {
    use super::*;

    #[test]
    fn c_registration_sites_produce_references_edges_the_756_scenario() {
        let project = TempProject::new("cg-fnref-c");
        project.write_lines(
            "driver.c",
            &[
                "struct ops { void (*recv_cb)(int); void (*send_cb)(int); };",
                "typedef void (*cb_t)(int);",
                "",
                "static void my_recv_cb(int x) { (void)x; }",
                "static void my_send_cb(int x) { (void)x; }",
                "",
                "void register_handler(void (*cb)(int)) { cb(1); }",
                "",
                "void direct_caller(void) { my_recv_cb(5); }",
                "",
                "void arg_registrar(void) { register_handler(my_recv_cb); }",
                "void addr_registrar(void) { register_handler(&my_recv_cb); }",
                "void assign_registrar(struct ops *o) { o->recv_cb = my_recv_cb; }",
                "",
                "static struct ops global_ops = { .recv_cb = my_recv_cb, .send_cb = my_send_cb };",
                "static cb_t cb_table[] = { my_recv_cb, my_send_cb };",
            ],
        );

        let mut cg = index(project.path());

        let into_recv = fn_ref_edges_into(&mut cg, "my_recv_cb");
        assert_names_eq(
            source_names(&mut cg, &into_recv),
            &[
                "addr_registrar",
                "arg_registrar",
                "assign_registrar",
                "driver.c",
            ],
        );

        // The direct call is still a `calls` edge, unchanged by this feature.
        let recv = cg
            .get_nodes_by_name("my_recv_cb")
            .into_iter()
            .next()
            .expect("my_recv_cb should be indexed");
        let call_edges = cg
            .get_incoming_edges(&recv.id)
            .into_iter()
            .filter(|edge| edge.kind == EdgeKind::Calls)
            .collect::<Vec<_>>();
        assert_names_eq(source_names(&mut cg, &call_edges), &["direct_caller"]);

        cg.destroy();
    }

    #[test]
    fn typescript_arg_object_array_member_assignment_forms() {
        let project = TempProject::new("cg-fnref-ts");
        project.write_lines(
            "main.ts",
            &[
                "export function targetCb(x: number): void { console.log(x); }",
                "function registerHandler(cb: (x: number) => void): void { cb(1); }",
                "",
                "export function argRegistrar(): void { registerHandler(targetCb); }",
                "export function timerRegistrar(): void { setTimeout(targetCb, 100); }",
                "export function objRegistrar(): unknown { return { recv: targetCb }; }",
                "export function arrRegistrar(): unknown { return [targetCb]; }",
                "",
                "class Emitter { cb: ((x: number) => void) | null = null; }",
                "export function assignRegistrar(e: Emitter): void { e.cb = targetCb; }",
                "",
                "interface Btn { on(ev: string, cb: () => void): void; }",
                "export class Comp {",
                "  handleClick(): void {}",
                "  wire(btn: Btn): void { btn.on(\"click\", this.handleClick); }",
                "}",
            ],
        );

        let mut cg = index(project.path());

        let target_edges = fn_ref_edges_into(&mut cg, "targetCb");
        assert_names_eq(
            source_names(&mut cg, &target_edges),
            &[
                "argRegistrar",
                "arrRegistrar",
                "assignRegistrar",
                "objRegistrar",
                "timerRegistrar",
            ],
        );
        // `this.handleClick` resolves class-scoped (#808): the target must be a
        // method of the enclosing class, in the same file.
        let handle_edges = fn_ref_edges_into(&mut cg, "handleClick");
        assert_names_eq(source_names(&mut cg, &handle_edges), &["wire"]);

        cg.destroy();
    }

    #[test]
    fn resolves_an_imported_callback_across_files_via_its_import() {
        let project = TempProject::new("cg-fnref-import");
        project.write(
            "handlers.ts",
            "export function onMessage(x: number): void { console.log(x); }\n",
        );
        project.write_lines(
            "wiring.ts",
            &[
                "import { onMessage } from './handlers';",
                "export function wire(bus: { on(cb: (x: number) => void): void }): void {",
                "  bus.on(onMessage);",
                "}",
            ],
        );

        let mut cg = index(project.path());

        let edges = fn_ref_edges_into(&mut cg, "onMessage");
        let names = source_names(&mut cg, &edges);
        assert_names_contains(&names, "wire");
        // The edge must target the handlers.ts definition.
        let target = cg
            .get_node(&edges.first().expect("fnRef edge should exist").target)
            .expect("edge target should be indexed");
        assert!(target.file_path.ends_with("handlers.ts"));

        cg.destroy();
    }

    #[test]
    fn decoy_ambiguous_cross_file_name_without_an_import_resolves_to_no_edge() {
        let project = TempProject::new("cg-fnref-decoy");
        // Two same-named functions in different files...
        project.write("a.ts", "export function process(x: number): void {}\n");
        project.write("b.ts", "export function process(x: number): void {}\n");
        // ...and a registrar that names `process` without importing it. The
        // name still passes the extraction gate only if imported/defined here;
        // it is neither, so this asserts the gate. Even if it leaked through,
        // the ambiguity rule (unique-only cross-file) must yield no edge.
        project.write(
            "c.ts",
            "export function wire(bus: { on(cb: unknown): void }, process: unknown): void { bus.on(process); }\n",
        );

        let mut cg = index(project.path());

        let edges = fn_ref_edges_into(&mut cg, "process");
        let names = source_names(&mut cg, &edges);
        assert!(
            !names.iter().any(|name| name == "wire"),
            "wire should not register as a function-ref source: {names:?}"
        );

        cg.destroy();
    }

    #[test]
    fn same_file_priority_a_same_file_definition_beats_a_same_named_decoy_elsewhere() {
        let project = TempProject::new("cg-fnref-samefile");
        project.write("decoy.c", "void my_cb(int x) { (void)x; }\n");
        project.write_lines(
            "real.c",
            &[
                "static void my_cb(int x) { (void)x; }",
                "void register_handler(void (*cb)(int)) { cb(1); }",
                "void wire(void) { register_handler(my_cb); }",
            ],
        );

        let mut cg = index(project.path());

        let wires = fn_ref_edges_into(&mut cg, "my_cb")
            .into_iter()
            .filter(|edge| {
                cg.get_node(&edge.source)
                    .is_some_and(|node| node.name == "wire")
            })
            .collect::<Vec<_>>();
        assert_eq!(wires.len(), 1);
        let target = cg
            .get_node(&wires[0].target)
            .expect("wire edge target should be indexed");
        assert!(target.file_path.ends_with("real.c"));

        cg.destroy();
    }

    #[test]
    fn kind_filter_a_class_passed_as_a_value_gets_no_function_ref_edge() {
        let project = TempProject::new("cg-fnref-kind");
        project.write_lines(
            "main.ts",
            &[
                "export class Strategy { run(): void {} }",
                "export function consume(x: unknown): void { void x; }",
                "export function wire(): void { consume(Strategy); }",
            ],
        );

        let mut cg = index(project.path());

        let strategy = cg
            .get_nodes_by_name("Strategy")
            .into_iter()
            .find(|node| node.kind == NodeKind::Class)
            .expect("Strategy class should be indexed");
        let fn_ref = cg
            .get_incoming_edges(&strategy.id)
            .into_iter()
            .filter(is_fn_ref)
            .collect::<Vec<_>>();
        assert_eq!(fn_ref.len(), 0);

        cg.destroy();
    }

    #[test]
    fn self_a_function_registering_itself_produces_no_self_loop() {
        let project = TempProject::new("cg-fnref-self");
        project.write_lines(
            "main.ts",
            &[
                "declare function schedule(cb: () => void): void;",
                "export function retry(): void { schedule(retry); }",
            ],
        );

        let mut cg = index(project.path());

        let retry = cg
            .get_nodes_by_name("retry")
            .into_iter()
            .next()
            .expect("retry should be indexed");
        let self_loops = cg
            .get_incoming_edges(&retry.id)
            .into_iter()
            .filter(|edge| edge.source == retry.id && is_fn_ref(edge))
            .collect::<Vec<_>>();
        assert_eq!(self_loops.len(), 0);

        cg.destroy();
    }

    #[test]
    fn cpp_member_pointers_resolve_scoped_bare_ids_are_free_function_only() {
        let project = TempProject::new("cg-fnref-cpp");
        project.write_lines(
            "widget.cpp",
            &[
                "struct Widget {",
                "  void on_click(int x);",
                "};",
                "void Widget::on_click(int x) { (void)x; }",
                "struct Decoy {",
                "  void on_click(int x);",
                "};",
                "void Decoy::on_click(int x) { (void)x; }",
                "void free_cb(int x) { (void)x; }",
                "void bare_fn(int x) { (void)x; }",
                "void reg(void* p) { (void)p; }",
                "void wire() {",
                "  auto p = &Widget::on_click;",
                "  reg(p);",
                "  reg(&free_cb);",
                "  reg(bare_fn);",
                "}",
                "struct Buf { char* out(); };",
                "void copy_to(void* out_) { (void)out_; }",
                "void caller(char* out) { copy_to(out); }",
            ],
        );

        let mut cg = index(project.path());

        // Qualified member pointer resolves to Widget::on_click specifically.
        let on_clicks = cg.get_nodes_by_name("on_click");
        let widget_on_click = on_clicks
            .iter()
            .find(|node| node.qualified_name.contains("Widget"))
            .expect("Widget::on_click should be indexed")
            .clone();
        let decoy_on_click = on_clicks
            .iter()
            .find(|node| node.qualified_name.contains("Decoy"))
            .expect("Decoy::on_click should be indexed")
            .clone();
        let into_widget = cg
            .get_incoming_edges(&widget_on_click.id)
            .into_iter()
            .filter(is_fn_ref)
            .collect::<Vec<_>>();
        assert_eq!(into_widget.len(), 1);
        assert_eq!(
            cg.get_node(&into_widget[0].source).map(|node| node.name),
            Some("wire".to_owned())
        );
        let into_decoy = cg
            .get_incoming_edges(&decoy_on_click.id)
            .into_iter()
            .filter(is_fn_ref)
            .collect::<Vec<_>>();
        assert_eq!(into_decoy.len(), 0);

        // Explicit &fn resolves; bare identifier in C++ args does not.
        let free_edges = fn_ref_edges_into(&mut cg, "free_cb");
        let free_sources = source_names(&mut cg, &free_edges);
        assert_names_contains(&free_sources, "wire");
        assert_eq!(fn_ref_edges_into(&mut cg, "bare_fn").len(), 0);

        // The local `out` param must not produce an edge to Buf::out.
        if let Some(out_method) = cg
            .get_nodes_by_name("out")
            .into_iter()
            .find(|node| node.kind == NodeKind::Method)
        {
            let out_refs = cg
                .get_incoming_edges(&out_method.id)
                .into_iter()
                .filter(is_fn_ref)
                .collect::<Vec<_>>();
            assert_eq!(out_refs.len(), 0);
        }

        cg.destroy();
    }

    #[test]
    fn pascal_event_wiring_addr_and_bare_args() {
        let project = TempProject::new("cg-fnref-pas");
        project.write_lines(
            "main.pas",
            &[
                "unit Main;",
                "interface",
                "type",
                "  TCallback = procedure(X: Integer);",
                "  THolder = class",
                "  public",
                "    OnFire: TCallback;",
                "    procedure Wire;",
                "  end;",
                "procedure TargetCb(X: Integer);",
                "procedure RegisterHandler(Cb: TCallback);",
                "procedure ArgRegistrar;",
                "procedure AddrRegistrar;",
                "implementation",
                "procedure TargetCb(X: Integer);",
                "begin",
                "  WriteLn(X);",
                "end;",
                "procedure RegisterHandler(Cb: TCallback);",
                "begin",
                "  Cb(1);",
                "end;",
                "procedure ArgRegistrar;",
                "begin",
                "  RegisterHandler(TargetCb);",
                "end;",
                "procedure AddrRegistrar;",
                "begin",
                "  RegisterHandler(@TargetCb);",
                "end;",
                "procedure THolder.Wire;",
                "begin",
                "  OnFire := TargetCb;",
                "end;",
                "end.",
            ],
        );

        let mut cg = index(project.path());

        let edges = fn_ref_edges_into(&mut cg, "TargetCb");
        assert_names_eq(
            source_names(&mut cg, &edges),
            &["AddrRegistrar", "ArgRegistrar", "Wire"],
        );

        cg.destroy();
    }

    #[test]
    fn this_member_scoping_this_x_resolves_only_to_enclosing_class_never_elsewhere() {
        let project = TempProject::new("cg-fnref-thisx");
        project.write_lines(
            "main.ts",
            &[
                "declare const bus: { on(ev: string, cb: () => void): void };",
                "export class Decoy { refresh(): void {} }",
                "export class Panel {",
                "  views: number[] = [];",
                "  refresh(): void {}",
                "  wire(): void {",
                "    bus.on(\"update\", this.refresh);",
                "    bus.on(\"data\", this.views as never);",
                "    bus.on(\"gone\", this.missing as never);",
                "  }",
                "}",
            ],
        );

        let mut cg = index(project.path());

        let refreshes = cg.get_nodes_by_name("refresh");
        let panel_refresh = refreshes
            .iter()
            .find(|node| node.qualified_name.contains("Panel"))
            .expect("Panel.refresh should be indexed")
            .clone();
        let decoy_refresh = refreshes
            .iter()
            .find(|node| node.qualified_name.contains("Decoy"))
            .expect("Decoy.refresh should be indexed")
            .clone();

        let into_panel = cg
            .get_incoming_edges(&panel_refresh.id)
            .into_iter()
            .filter(is_fn_ref)
            .collect::<Vec<_>>();
        assert_eq!(into_panel.len(), 1);
        assert_eq!(
            cg.get_node(&into_panel[0].source).map(|node| node.name),
            Some("wire".to_owned())
        );
        let into_decoy = cg
            .get_incoming_edges(&decoy_refresh.id)
            .into_iter()
            .filter(is_fn_ref)
            .collect::<Vec<_>>();
        assert_eq!(into_decoy.len(), 0);

        // The property and the unknown member produce nothing.
        if let Some(views) = cg
            .get_nodes_by_name("views")
            .into_iter()
            .find(|node| node.kind == NodeKind::Property)
        {
            let view_refs = cg
                .get_incoming_edges(&views.id)
                .into_iter()
                .filter(is_fn_ref)
                .collect::<Vec<_>>();
            assert_eq!(view_refs.len(), 0);
        }

        cg.destroy();
    }

    #[test]
    fn inherited_this_x_resolves_on_a_supertype_via_second_pass_never_unrelated() {
        let project = TempProject::new("cg-fnref-inherit");
        project.write(
            "base.ts",
            "export class FormBase { handleSubmit(): void {} }\n",
        );
        project.write(
            "unrelated.ts",
            "export class Unrelated { handleSubmit(): void {} }\n",
        );
        project.write_lines(
            "login.ts",
            &[
                "import { FormBase } from './base';",
                "declare const bus: { on(ev: string, cb: () => void): void };",
                "export class LoginForm extends FormBase {",
                "  wire(): void { bus.on(\"submit\", this.handleSubmit); }",
                "}",
            ],
        );

        let mut cg = index(project.path());

        let handle_submits = cg.get_nodes_by_name("handleSubmit");
        let base_method = handle_submits
            .iter()
            .find(|node| node.qualified_name.contains("FormBase"))
            .expect("FormBase.handleSubmit should be indexed")
            .clone();
        let unrelated_method = handle_submits
            .iter()
            .find(|node| node.qualified_name.contains("Unrelated"))
            .expect("Unrelated.handleSubmit should be indexed")
            .clone();

        let into_base = cg
            .get_incoming_edges(&base_method.id)
            .into_iter()
            .filter(is_fn_ref)
            .collect::<Vec<_>>();
        assert_eq!(into_base.len(), 1);
        assert_eq!(
            cg.get_node(&into_base[0].source).map(|node| node.name),
            Some("wire".to_owned())
        );
        let into_unrelated = cg
            .get_incoming_edges(&unrelated_method.id)
            .into_iter()
            .filter(is_fn_ref)
            .collect::<Vec<_>>();
        assert_eq!(into_unrelated.len(), 0);

        cg.destroy();
    }

    #[test]
    fn java_type_method_cross_file_this_super_scoped_variable_yields_nothing() {
        let project = TempProject::new("cg-fnref-java");
        project.write_lines(
            "Handlers.java",
            &[
                "package com.example;",
                "public class Handlers {",
                "    public static void onMessage(int x) { System.out.println(x); }",
                "}",
            ],
        );
        project.write_lines(
            "BaseForm.java",
            &[
                "package com.example;",
                "public class BaseForm {",
                "    void baseHandler(int x) {}",
                "}",
            ],
        );
        project.write_lines(
            "Main.java",
            &[
                "package com.example;",
                "import com.example.Handlers;",
                "import java.util.function.IntConsumer;",
                "public class Main extends BaseForm {",
                "    static void registerHandler(IntConsumer cb) { cb.accept(1); }",
                "    void run0() {}",
                "    void crossFile() { registerHandler(Handlers::onMessage); }",
                "    void thisRef() { registerHandler(this::run0); }",
                "    void superRef() { registerHandler(super::baseHandler); }",
                "    void varRef(Main m) { registerHandler(m::run0); }",
                "}",
            ],
        );

        let mut cg = index(project.path());

        let on_message = fn_ref_edges_into(&mut cg, "onMessage");
        assert_names_eq(source_names(&mut cg, &on_message), &["crossFile"]);
        let base_handler = fn_ref_edges_into(&mut cg, "baseHandler");
        assert_names_eq(source_names(&mut cg, &base_handler), &["superRef"]);
        // this::run0 resolves class-scoped; m::run0 (variable receiver) must
        // not add a second edge, exactly one source.
        let run0 = fn_ref_edges_into(&mut cg, "run0");
        assert_names_eq(source_names(&mut cg, &run0), &["thisRef"]);

        cg.destroy();
    }

    #[test]
    fn kotlin_companion_object_refs_resolve_cross_file_without_imports_decoy_untouched() {
        let project = TempProject::new("cg-fnref-ktcomp");
        // Same package, no imports: the Java/Kotlin reality the name gate
        // cannot see, which is why qualified `Type::member` candidates skip it.
        project.write_lines(
            "Handlers.kt",
            &[
                "class KtHandlers {",
                "  companion object {",
                "    fun handle(x: Int) {}",
                "  }",
                "}",
                "class Decoy {",
                "  companion object {",
                "    fun handle(x: Int) {}",
                "  }",
                "}",
            ],
        );
        project.write_lines(
            "Wirer.kt",
            &[
                "fun register(cb: Any) {}",
                "class Wirer {",
                "  fun wire() { register(KtHandlers::handle) }",
                "}",
            ],
        );

        let mut cg = index(project.path());

        let handles = cg.get_nodes_by_name("handle");
        let target = handles
            .iter()
            .find(|node| node.qualified_name.contains("KtHandlers"))
            .expect("KtHandlers.handle should be indexed")
            .clone();
        let decoy = handles
            .iter()
            .find(|node| node.qualified_name.contains("Decoy"))
            .expect("Decoy.handle should be indexed")
            .clone();
        let into = cg
            .get_incoming_edges(&target.id)
            .into_iter()
            .filter(is_fn_ref)
            .collect::<Vec<_>>();
        assert_eq!(into.len(), 1);
        assert_eq!(
            cg.get_node(&into[0].source).map(|node| node.name),
            Some("wire".to_owned())
        );
        let into_decoy = cg
            .get_incoming_edges(&decoy.id)
            .into_iter()
            .filter(is_fn_ref)
            .collect::<Vec<_>>();
        assert_eq!(into_decoy.len(), 0);

        cg.destroy();
    }

    #[test]
    fn swift_scoping_bare_ids_hit_enclosing_type_methods_top_level_hits_functions_only() {
        let project = TempProject::new("cg-fnref-swiftscope");
        project.write_lines(
            "main.swift",
            &[
                "func register(_ cb: (Int) -> Void) { cb(1) }",
                "class Monitor {",
                "  func report(_ x: Int) {}",
                "  func wire() { register(report) }",
                "}",
                "class Other {",
                "  func use(report: (Int) -> Void) { register(report) }",
                "}",
                "func topLevel() { register(report) }",
            ],
        );

        let mut cg = index(project.path());

        let edges = fn_ref_edges_into(&mut cg, "report");
        assert_names_eq(source_names(&mut cg, &edges), &["wire"]);

        cg.destroy();
    }

    #[test]
    fn c_ungated_tables_command_table_names_handlers_defined_in_other_files() {
        let project = TempProject::new("cg-fnref-ctable");
        // Handler defined in its own file...
        project.write("t_string.c", "void getCommand(int c) { (void)c; }\n");
        // ...and registered in a table in another file, with no import mechanism.
        project.write_lines(
            "server.c",
            &[
                "struct cmd { const char *name; void (*proc)(int); };",
                "static struct cmd commandTable[] = {",
                "  { \"get\", getCommand },",
                "};",
            ],
        );
        // Ambiguity safety: two files define dupCmd; a third table references
        // it and therefore gets no edge (unique-or-drop).
        project.write("dup_a.c", "void dupCmd(int c) { (void)c; }\n");
        project.write("dup_b.c", "void dupCmd(int c) { (void)c; }\n");
        project.write_lines(
            "other.c",
            &[
                "struct cmd2 { void (*proc)(int); };",
                "static struct cmd2 otherTable[] = { { dupCmd } };",
            ],
        );

        let mut cg = index(project.path());

        // Cross-file unique handler resolves from the table's file.
        let into_get = fn_ref_edges_into(&mut cg, "getCommand");
        assert_names_eq(source_names(&mut cg, &into_get), &["server.c"]);
        let target = cg
            .get_node(
                &into_get
                    .first()
                    .expect("getCommand edge should exist")
                    .target,
            )
            .expect("getCommand target should be indexed");
        assert!(target.file_path.ends_with("t_string.c"));

        // Ambiguous handler resolves to nothing: silent beats wrong.
        assert_eq!(fn_ref_edges_into(&mut cg, "dupCmd").len(), 0);

        cg.destroy();
    }

    #[test]
    fn php_hof_string_callables_this_and_class_arrays_non_hof_strings_ignored() {
        let project = TempProject::new("cg-fnref-php");
        project.write(
            "handlers.php",
            "<?php\nfunction cmp_items($a, $b) { return $a <=> $b; }\n",
        );
        project.write_lines(
            "main.php",
            &[
                "<?php",
                "class Saver {",
                "    public function onSave($x) {}",
                "    public function wire() {",
                "        register_shutdown_function([$this, 'onSave']);",
                "    }",
                "}",
                "class Loader {",
                "    public static function load($cls) {}",
                "}",
                "function sorter($items) {",
                "    usort($items, 'cmp_items');",
                "    spl_autoload_register([Loader::class, 'load']);",
                "    some_random_fn('cmp_items');",
                "    return $items;",
                "}",
            ],
        );

        let mut cg = index(project.path());

        // Exactly one source for cmp_items: the usort site, not some_random_fn.
        let cmp_edges = fn_ref_edges_into(&mut cg, "cmp_items");
        assert_names_eq(source_names(&mut cg, &cmp_edges), &["sorter"]);
        let on_save_edges = fn_ref_edges_into(&mut cg, "onSave");
        assert_names_eq(source_names(&mut cg, &on_save_edges), &["wire"]);
        let load_edges = fn_ref_edges_into(&mut cg, "load");
        assert_names_eq(source_names(&mut cg, &load_edges), &["sorter"]);

        cg.destroy();
    }

    #[test]
    fn ruby_hooks_before_action_rescue_from_symbols_resolve_class_scoped_inherited() {
        let project = TempProject::new("cg-fnref-rubyhooks");
        project.write_lines(
            "posts_controller.rb",
            &[
                "class ApplicationController",
                "  def authenticate; end",
                "end",
                "",
                "class PostsController < ApplicationController",
                "  before_action :authenticate",
                "  after_save :reindex",
                "  validates :title, presence: true",
                "  rescue_from StandardError, with: :render_500",
                "",
                "  def reindex; end",
                "  def render_500; end",
                "  def title; end",
                "end",
            ],
        );

        let mut cg = index(project.path());

        let auth = fn_ref_edges_into(&mut cg, "authenticate");
        assert_eq!(auth.len(), 1);
        assert!(
            cg.get_node(&auth[0].target)
                .expect("authenticate target should be indexed")
                .qualified_name
                .contains("ApplicationController")
        );

        assert_eq!(fn_ref_edges_into(&mut cg, "reindex").len(), 1);
        assert_eq!(fn_ref_edges_into(&mut cg, "render_500").len(), 1);
        // `validates :title` names an attribute; the same-named method must get
        // no registration edge.
        assert_eq!(fn_ref_edges_into(&mut cg, "title").len(), 0);

        cg.destroy();
    }

    #[test]
    fn drain_resolvable_function_ref_rows_leave_unresolved_refs_reindex_is_stable() {
        let project = TempProject::new("cg-fnref-drain");
        project.write_lines(
            "main.c",
            &[
                "static void cb_a(int x) { (void)x; }",
                "void reg(void (*cb)(int)) { cb(1); }",
                "void wire(void) { reg(cb_a); }",
            ],
        );

        let mut cg = index(project.path());
        let stats1 = cg.get_stats();

        // No function_ref rows may linger for resolvable names: the batched
        // resolver must drain them.
        let db_path = get_code_graph_dir(project.path()).join("rustcodegraph.db");
        let conn = Connection::open(&db_path)
            .unwrap_or_else(|err| panic!("failed to open {}: {err}", db_path.display()));
        let leftover_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM unresolved_refs WHERE reference_kind = 'function_ref'",
                [],
                |row| row.get(0),
            )
            .expect("unresolved_refs query should run");
        assert_eq!(leftover_count, 0);

        // Re-index: identical node/edge counts (idempotent, no accumulation).
        let result = cg.index_all(IndexOptions::default());
        assert!(
            result.success,
            "re-index should succeed, errors: {:?}",
            result.errors
        );
        let stats2 = cg.get_stats();
        assert_eq!(stats2.node_count, stats1.node_count);
        assert_eq!(stats2.edge_count, stats1.edge_count);

        let cb_edges = fn_ref_edges_into(&mut cg, "cb_a");
        assert_names_eq(source_names(&mut cg, &cb_edges), &["wire"]);

        cg.destroy();
    }
}
