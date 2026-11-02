use rustcodegraph::mcp::tools::ToolHandler;
use rustcodegraph::{CodeGraph, IndexOptions};
use serde_json::json;

fn probe(query: &str) -> String {
    let root = std::env::temp_dir().join(format!(
        "rcg32-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("flow.ts"), "export function mutateElement() { triggerUpdate(); }\nfunction triggerUpdate() { renderStaticScene(); }\nfunction renderStaticScene() { return 1; }\n").unwrap();
    let mut graph = CodeGraph::init_sync(&root).unwrap();
    assert!(graph.index_all(IndexOptions::default()).success);
    let mut handler = ToolHandler::new(true);
    handler.set_default_code_graph(&graph);
    let result = handler.execute(
        "rustcodegraph_explore",
        json!({"query":query}).as_object().unwrap(),
    );
    assert_ne!(result.is_error, Some(true));
    let text = result.content.into_iter().map(|c| c.text).collect();
    handler.close_all();
    graph.destroy();
    std::fs::remove_dir_all(root).unwrap();
    text
}

#[test]
fn case_032_connected_control_retains_source() {
    let text = probe("mutateElement renderStaticScene");
    assert!(text.contains("triggerUpdate"), "{text}");
    assert!(text.contains("return 1"), "{text}");
}
#[test]
fn case_032_missing_endpoint_does_not_invent_source() {
    let text = probe("nonexistent_endpoint_032");
    assert!(!text.contains("return 1"), "{text}");
}
#[test]
fn case_032_unrelated_query_does_not_claim_control_flow() {
    let text = probe("unrelated_symbol_032 another_absent_032");
    assert!(!text.contains("triggerUpdate();"), "{text}");
}

#[test]
fn case_032_cli_indexes_exported_arrow_functions() {
    let root = std::env::temp_dir().join(format!("rcg32-cli-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("flow.ts"), "export const mutateElement = <T>(value: T) => { renderStaticScene(value); };\nexport const renderStaticScene = (value: unknown) => { return value; };\n").unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_rustcodegraph"))
        .args(["init", "-i", root.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let conn = rusqlite::Connection::open(root.join(".rustcodegraph/rustcodegraph.db")).unwrap();
    let count: i64 = conn.query_row("SELECT count(*) FROM nodes WHERE kind='function' AND name IN ('mutateElement', 'renderStaticScene')", [], |r| r.get(0)).unwrap();
    drop(conn);
    std::fs::remove_dir_all(root).unwrap();
    assert_eq!(
        count, 2,
        "CLI must retain the same arrow-function endpoints as library indexing"
    );
}

#[test]
fn case_032_stdio_explore_returns_library_source() {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let root = std::env::temp_dir().join(format!("rcg32-stdio-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("flow.ts"), "export function mutateElement() { renderStaticScene(); }\nfunction renderStaticScene() { return 32032; }\n").unwrap();
    let mut graph = CodeGraph::init_sync(&root).unwrap();
    assert!(graph.index_all(IndexOptions::default()).success);
    graph.close();
    let mut child = Command::new(env!("CARGO_BIN_EXE_rustcodegraph"))
        .args(["serve", "--mcp", "--path", root.to_str().unwrap()])
        .env("RUSTCODEGRAPH_NO_DAEMON", "1")
        .env("RUSTCODEGRAPH_WATCH", "0")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    for message in [
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"rustcodegraph_explore","arguments":{"query":"mutateElement renderStaticScene"}}}),
    ] {
        writeln!(input, "{message}").unwrap();
    }
    drop(input);
    let output = child.wait_with_output().unwrap();
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "{output:?}");
    assert!(text.contains("return 32032"), "{text}");
    std::fs::remove_dir_all(root).unwrap();
}
