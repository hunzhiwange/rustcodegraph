use rustcodegraph::{
    CodeGraph,
    types::{Edge, EdgeProvenance},
};

fn check(files: &[(&str, &str)], run: impl FnOnce(&mut CodeGraph, &std::path::Path)) {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let root = Cleanup(std::env::temp_dir().join(format!(
        "rcg363-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    )));
    std::fs::create_dir_all(&root.0).unwrap();
    for (path, source) in files {
        let path = root.0.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, source).unwrap();
    }
    let mut g = CodeGraph::init_sync(&root.0).unwrap();
    let r = g.index_all(Default::default());
    assert!(r.success, "{:?}", r.errors);
    run(&mut g, &root.0);
    g.close();
}
fn edges(g: &mut CodeGraph, file: &str, name: &str) -> Vec<Edge> {
    let node = g
        .get_nodes_in_file(file)
        .into_iter()
        .find(|n| n.name == name)
        .unwrap();
    g.get_outgoing_edges(&node.id)
        .into_iter()
        .filter(|e| e.provenance == Some(EdgeProvenance::Heuristic))
        .collect()
}
fn links(g: &mut CodeGraph, file: &str, name: &str) -> Vec<Edge> {
    edges(g, file, name)
        .into_iter()
        .filter(|e| {
            e.metadata
                .as_ref()
                .is_some_and(|m| m["synthesizedBy"] == "tauri-event")
        })
        .collect()
}
const RUST: &str = r#"use tauri::{AppHandle, Emitter, Listener};
fn install(app: tauri::AppHandle) {
    app.listen("request", receive);
}
fn receive(event: tauri::Event) { finish(); }
fn finish() {}
fn publish(app: tauri::AppHandle) {
    app.emit("response", ()).unwrap();
}
"#;
const FRONT: &str = r#"import { emit as send, listen as subscribe } from '@tauri-apps/api/event';
export function click() { send('request'); }
export function installUi() { subscribe('response', update); }
export function update() { paint(); }
export function paint() {}
"#;
#[test]
fn case_363_bidirectional_flow_and_incremental_removal() {
    check(
        &[("src-tauri/src/lib.rs", RUST), ("src/app.ts", FRONT)],
        |g, root| {
            for (file, name, target, tier) in [
                ("src/app.ts", "click", "receive", "client→server"),
                ("src-tauri/src/lib.rs", "publish", "update", "server→client"),
            ] {
                let e = links(g, file, name);
                assert_eq!(e.len(), 1, "{name}: {e:?}");
                assert_eq!(g.get_node(&e[0].target).unwrap().name, target);
                let m = e[0].metadata.as_ref().unwrap();
                assert_eq!(m["tier"], tier);
                assert!(m["registeredAt"].as_str().unwrap().contains(':'));
            }
            for (query, expected) in [
                (
                    "publish update paint",
                    [
                        "publish",
                        "update",
                        "paint",
                        "tauri-event",
                        "server→client",
                        "registeredAt",
                    ],
                ),
                (
                    "click receive finish",
                    [
                        "click",
                        "receive",
                        "finish",
                        "tauri-event",
                        "client→server",
                        "registeredAt",
                    ],
                ),
            ] {
                let mut handler = rustcodegraph::mcp::tools::ToolHandler::new(true);
                handler.set_default_code_graph(g);
                let response = handler.execute(
                    "rustcodegraph_explore",
                    serde_json::json!({"query":query}).as_object().unwrap(),
                );
                let value = serde_json::to_value(response).unwrap();
                let text = value["content"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter_map(|c| c["text"].as_str())
                    .collect::<Vec<_>>()
                    .join("\n");
                let flow = text
                    .split("## Flow")
                    .nth(1)
                    .expect(&text)
                    .split("\n## ")
                    .next()
                    .unwrap();
                for expected in expected {
                    assert!(flow.contains(expected), "{flow}");
                }
            }
            let count = g.get_nodes_in_file("src-tauri/src/lib.rs").len();
            assert!(g.index_all(Default::default()).success);
            assert_eq!(links(g, "src/app.ts", "click").len(), 1);
            assert_eq!(g.get_nodes_in_file("src-tauri/src/lib.rs").len(), count);
            std::fs::write(
                root.join("src-tauri/src/lib.rs"),
                RUST.replace("app.listen(\"request\", receive);", ""),
            )
            .unwrap();
            assert_eq!(g.sync(Default::default()).files_modified, 1);
            assert!(links(g, "src/app.ts", "click").is_empty());
            std::fs::write(
                root.join("src/app.ts"),
                FRONT.replace("subscribe('response', update);", ""),
            )
            .unwrap();
            assert_eq!(g.sync(Default::default()).files_modified, 1);
            assert!(links(g, "src-tauri/src/lib.rs", "publish").is_empty());
        },
    );
}
#[test]
fn case_363_multi_app_missing_and_dynamic_events() {
    check(
        &[
            ("src-tauri/src/lib.rs", RUST),
            ("a/src-tauri/src/lib.rs", RUST),
            ("a/src/app.ts", FRONT),
            ("b/src-tauri/src/lib.rs", "fn empty() {}"),
            ("b/src/app.ts", FRONT),
            ("a/src-tauri/src/other.rs", "fn receive() {}"),
            (
                "a/src/dynamic.ts",
                "import { emit } from '@tauri-apps/api/event';\nfunction dynamic(key: string) { emit(key); emit('request' + key); emit('absent'); }",
            ),
            (
                "a/src/fake.ts",
                "function emit(key: string) {}\nfunction fake() { emit('request'); }",
            ),
        ],
        |g, _| {
            let e = links(g, "a/src/app.ts", "click");
            assert_eq!(e.len(), 1);
            assert_eq!(
                g.get_node(&e[0].target).unwrap().file_path,
                "a/src-tauri/src/lib.rs"
            );
            for (file, name) in [
                ("b/src/app.ts", "click"),
                ("a/src/dynamic.ts", "dynamic"),
                ("a/src/fake.ts", "fake"),
            ] {
                assert!(links(g, file, name).is_empty());
            }
        },
    );
}
#[test]
fn case_363_shadowed_import_and_non_tauri_receiver() {
    check(
        &[
            ("src-tauri/src/lib.rs", RUST),
            (
                "src/app.ts",
                "import { emit } from '@tauri-apps/api/event';\nfunction shadow(emit: any) { emit('request'); }",
            ),
            (
                "src-tauri/src/fake.rs",
                "fn fake(app: Other) { app.emit(\"response\", ()); }",
            ),
            ("src/ui.ts", FRONT),
        ],
        |g, _| {
            assert!(links(g, "src/app.ts", "shadow").is_empty());
            assert!(links(g, "src-tauri/src/fake.rs", "fake").is_empty());
        },
    );
}

#[test]
fn case_363_namespace_once_and_inline_listeners() {
    let rust = "use tauri::{AppHandle, Emitter, Listener};\nfn register(app: AppHandle) {\napp.once(\"request\", |event| { finish(); });\n}\nfn finish() {}\nfn publish(app: AppHandle) { app.emit(\"response\", ()); }";
    let front = "import * as events from '@tauri-apps/api/event';\nfunction click() { events.emit('request'); }\nfunction subscribe() { events.once('response', () => { paint(); }); }\nfunction paint() {}";
    check(
        &[("src-tauri/src/lib.rs", rust), ("src/app.ts", front)],
        |g, _| {
            let e = links(g, "src/app.ts", "click");
            assert_eq!(e.len(), 1);
            assert_eq!(g.get_node(&e[0].target).unwrap().name, "register");
            let e = links(g, "src-tauri/src/lib.rs", "publish");
            assert_eq!(e.len(), 1);
            assert_eq!(g.get_node(&e[0].target).unwrap().name, "subscribe");
        },
    );
}
