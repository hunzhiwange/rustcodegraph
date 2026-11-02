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
