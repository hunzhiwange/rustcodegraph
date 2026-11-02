use rustcodegraph::mcp::tools::ToolHandler;
use rustcodegraph::{CodeGraph, IndexOptions};
use serde_json::json;
fn explore(files: &[(&str, String)], query: &str, cap: usize) -> String {
    let root = std::env::temp_dir().join(format!(
        "rcg26-{}-{:?}",
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
fn case_026_tail_member_complete() {
    let body = format!(
        "{}\nexport function target() {{\n return '完整尾部';\n}}",
        "export const padding = 0;\n".repeat(1600)
    );
    let text = explore(&[("large.ts", body)], "target", 1);
    assert!(text.contains("完整尾部"), "target body omitted");
}
#[test]
fn case_026_small_source_unchanged() {
    let text = explore(
        &[(
            "small.ts",
            "export function target() { return 'small-body'; }".into(),
        )],
        "target",
        1,
    );
    assert!(text.contains("small-body"));
}
#[test]
fn case_026_oversize_member_preserves_whole_signature() {
    let signature = format!(
        "export function target({}) {{",
        (0..500)
            .map(|i| format!("arg{i}: string"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let body = format!(
        "{signature}\n{}\n}}",
        " console.log('中文');\n".repeat(2000)
    );
    let text = explore(&[("large.ts", body)], "target", 1);
    assert!(text.contains(&signature), "signature cut");
}
