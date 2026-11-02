use rustcodegraph::mcp::tools::ToolHandler;
use rustcodegraph::{CodeGraph, IndexOptions};
use serde_json::json;
fn explore(files: &[(&str, String)], query: &str, cap: usize) -> String {
    let root = std::env::temp_dir().join(format!(
        "rcg25-{}-{:?}",
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
fn business() -> String {
    (0..20)
        .map(|i| format!("export function business{i}() {{ return {i}; }}\n"))
        .collect()
}
#[test]
fn case_025_low_score_retains_location_without_source_slot() {
    let text = explore(
        &[
            ("generated.ts", "export const crud = 123456;".into()),
            ("business.ts", business()),
        ],
        "crud business",
        1,
    );
    assert!(text.contains("#### business.ts"), "{text}");
    assert!(
        text.contains("generated.ts:1") && text.contains("crud"),
        "{text}"
    );
    assert!(!text.contains("123456"), "{text}");
}
#[test]
fn case_025_single_low_score_file_still_has_source() {
    let text = explore(
        &[("generated.ts", "export const crud = 123456;".into())],
        "crud",
        1,
    );
    assert!(text.contains("123456"), "{text}");
}
#[test]
fn case_025_peer_matches_keep_source() {
    let text = explore(
        &[
            ("a.ts", "export function alpha() { return 123456; }".into()),
            ("b.ts", "export function beta() { return 654321; }".into()),
        ],
        "alpha beta",
        2,
    );
    assert!(text.contains("123456") && text.contains("654321"), "{text}");
}
