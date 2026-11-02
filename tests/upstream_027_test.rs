use rustcodegraph::mcp::tools::ToolHandler;
use rustcodegraph::{CodeGraph, IndexOptions};
use serde_json::json;
fn explore(files: &[(&str, String)], query: &str, cap: usize) -> String {
    let root = std::env::temp_dir().join(format!(
        "rcg27-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    for (path, body) in files {
        std::fs::write(root.join(path), body).unwrap();
    }
    let mut cg = CodeGraph::init_sync(&root).unwrap();
    assert!(cg.index_all(IndexOptions::default()).success);
    let mut handler = ToolHandler::new(true);
    handler.set_default_code_graph(&cg);
    let result = handler.execute(
        "rustcodegraph_explore",
        json!({"query":query,"maxFiles":cap}).as_object().unwrap(),
    );
    let text = serde_json::to_value(result).unwrap().to_string();
    handler.close_all();
    cg.destroy();
    std::fs::remove_dir_all(root).unwrap();
    text
}
#[test]
fn case_027_bridge_source_survives() {
    let body = format!(
        "{}\nexport function start() {{ bridge(); }}\nfunction bridge() {{ finish(); console.log('BRIDGE_BODY'); }}\nexport function finish() {{ console.log('FINISH_BODY'); }}",
        "export const padding = 0;\n".repeat(1600)
    );
    let text = explore(&[("flow.ts", body)], "start finish", 1);
    assert!(
        text.contains("BRIDGE_BODY"),
        "bridge source missing: {text}"
    );
    assert!(text.contains("FINISH_BODY"));
}
#[test]
fn case_027_no_path_keeps_named_source() {
    let text=explore(&[("single.ts","export function start() { return 'NAMED_BODY'; }\nexport function finish() { return 2; }".into())],"start finish",1);
    assert!(text.contains("NAMED_BODY"));
}
#[test]
fn case_027_single_symbol_unchanged() {
    let text = explore(
        &[(
            "single.ts",
            "export function start() { return 'SINGLE_BODY'; }".into(),
        )],
        "start",
        1,
    );
    assert!(text.contains("SINGLE_BODY"));
}
