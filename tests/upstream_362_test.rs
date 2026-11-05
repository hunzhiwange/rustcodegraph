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
        "rcg362-{}-{}",
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
const CORE: &str = r#"#[tauri::command]
fn greet() { finish(); }
fn finish() {}
#[tauri::command]
fn unregistered() {}
fn plain() {}
fn run() { tauri::Builder::default().invoke_handler(tauri::generate_handler![greet, plain]); }
"#;
fn links(g: &mut CodeGraph, file: &str, name: &str) -> Vec<Edge> {
    edges(g, file, name)
        .into_iter()
        .filter(|e| {
            e.metadata
                .as_ref()
                .is_some_and(|m| m["synthesizedBy"] == "tauri-command")
        })
        .collect()
}
#[test]
fn case_362_registered_command_and_repeat_index() {
    check(
        &[
            ("src-tauri/src/lib.rs", CORE),
            (
                "src/app.ts",
                "import { invoke as call } from '@tauri-apps/api/core';\nexport function click() { return call<string>('greet'); }",
            ),
        ],
        |g, root| {
            let e = links(g, "src/app.ts", "click");
            assert_eq!(e.len(), 1, "registered cross-language command: {e:?}");
            assert_eq!(g.get_node(&e[0].target).unwrap().name, "greet");
            assert_eq!(
                e[0].metadata.as_ref().unwrap()["registeredAt"],
                "src-tauri/src/lib.rs:7"
            );
            assert_eq!(e[0].line, Some(2));
            assert_eq!(e[0].kind, rustcodegraph::types::EdgeKind::Calls);
            let mut handler = rustcodegraph::mcp::tools::ToolHandler::new(true);
            handler.set_default_code_graph(g);
            let response = handler.execute(
                "rustcodegraph_explore",
                serde_json::json!({"query":"click greet finish"})
                    .as_object()
                    .unwrap(),
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
            assert!(
                flow.contains("click") && flow.contains("greet") && flow.contains("finish"),
                "{flow}"
            );
            let count = g.get_nodes_in_file("src-tauri/src/lib.rs").len();
            assert!(g.index_all(Default::default()).success);
            assert_eq!(links(g, "src/app.ts", "click").len(), 1);
            assert_eq!(g.get_nodes_in_file("src-tauri/src/lib.rs").len(), count);
            std::fs::write(
                root.join("src-tauri/src/lib.rs"),
                CORE.replace("[greet, plain]", "[plain]"),
            )
            .unwrap();
            assert_eq!(g.sync(Default::default()).files_modified, 1);
            assert!(links(g, "src/app.ts", "click").is_empty());
        },
    );
}
#[test]
fn case_362_missing_registration_or_command_attribute() {
    check(
        &[
            ("src-tauri/src/lib.rs", CORE),
            (
                "src/app.ts",
                "import { invoke } from '@tauri-apps/api/core';\nfunction absent() { invoke('unregistered'); invoke('plain'); }",
            ),
        ],
        |g, _| {
            assert!(links(g, "src/app.ts", "absent").is_empty());
        },
    );
}
#[test]
fn case_362_unproven_frontend_calls() {
    check(
        &[
            ("src-tauri/src/lib.rs", CORE),
            (
                "src/local.ts",
                "function invoke(key: string) {}\nfunction local() { invoke('greet'); }",
            ),
            (
                "src/app.ts",
                "import { invoke } from '@tauri-apps/api/core';\nfunction shadow(invoke: any) { invoke('greet'); }\nfunction other() {}",
            ),
            (
                "src/dynamic.ts",
                "import { invoke } from '@tauri-apps/api/core';\nfunction dynamic(key: string) { invoke(key); invoke('greet' + key); invoke(`greet_${key}`); }",
            ),
        ],
        |g, _| {
            for (file, name) in [
                ("src/local.ts", "local"),
                ("src/app.ts", "shadow"),
                ("src/dynamic.ts", "dynamic"),
            ] {
                assert!(links(g, file, name).is_empty());
            }
        },
    );
}
#[test]
fn case_362_fake_registration_is_not_evidence() {
    let source = r##"#[tauri::command]
fn greet() {}
fn run() {
    // tauri::generate_handler![greet]
    let s = "tauri::generate_handler![greet]";
    other_handler![greet];
}"##;
    check(
        &[
            ("src-tauri/src/lib.rs", source),
            (
                "src/app.ts",
                "import { invoke } from '@tauri-apps/api/core';\nfunction click() { invoke('greet'); }",
            ),
        ],
        |g, _| assert!(links(g, "src/app.ts", "click").is_empty()),
    );
}

#[test]
fn case_362_qualified_registration_namespace_and_app_isolation() {
    check(
        &[
            ("src-tauri/src/lib.rs", CORE),
            (
                "a/src-tauri/src/lib.rs",
                "mod commands;\nfn run() { tauri::generate_handler![commands::greet]; }",
            ),
            (
                "a/src-tauri/src/commands.rs",
                "#[tauri::command]\npub fn greet() {}",
            ),
            (
                "a/src-tauri/src/unrelated.rs",
                "#[tauri::command]\npub fn greet() {}",
            ),
            (
                "a/src/app.ts",
                "import * as core from '@tauri-apps/api/core';\nfunction click() { return core.invoke('greet'); }",
            ),
            ("b/src-tauri/src/lib.rs", "#[tauri::command]\nfn greet() {}"),
            (
                "b/src/app.ts",
                "import { invoke } from '@tauri-apps/api/core';\nfunction click() { return invoke('greet'); }",
            ),
        ],
        |g, _| {
            let e = links(g, "a/src/app.ts", "click");
            assert_eq!(e.len(), 1);
            assert_eq!(
                g.get_node(&e[0].target).unwrap().file_path,
                "a/src-tauri/src/commands.rs"
            );
            assert!(links(g, "b/src/app.ts", "click").is_empty());
        },
    );
}
#[test]
fn case_362_conflicting_registered_targets_are_silent() {
    check(
        &[
            (
                "src-tauri/src/lib.rs",
                "mod one; mod two;\nfn run() { tauri::generate_handler![one::greet, two::greet]; }",
            ),
            (
                "src-tauri/src/one.rs",
                "#[tauri::command]\npub fn greet() {}",
            ),
            (
                "src-tauri/src/two.rs",
                "#[tauri::command]\npub fn greet() {}",
            ),
            (
                "src/app.ts",
                "import { invoke } from '@tauri-apps/api/core';\nfunction click() { invoke('greet'); }",
            ),
        ],
        |g, _| assert!(links(g, "src/app.ts", "click").is_empty()),
    );
}

#[test]
fn case_362_nested_names_do_not_register_a_top_level_namesake() {
    for source in [
        "#[tauri::command]\nfn greet() {}\nmod inner { fn greet() {} fn run() { tauri::generate_handler![greet]; } }",
        "#[tauri::command]\nfn greet() {}\nfn run() { fn greet() {} tauri::generate_handler![greet]; }",
    ] {
        check(
            &[
                ("src-tauri/src/lib.rs", source),
                (
                    "src/app.ts",
                    "import { invoke } from '@tauri-apps/api/core';\nfunction click() { invoke('greet'); }",
                ),
            ],
            |g, _| assert!(links(g, "src/app.ts", "click").is_empty()),
        );
    }
}
