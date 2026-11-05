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
    assert_ne!(result.is_error, Some(true));
    let text = result
        .content
        .into_iter()
        .map(|c| c.text)
        .collect::<Vec<_>>()
        .join("\n");
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
    // A connected query exercises flow snippet selection and its hard file cap.
    let signature = format!(
        "export function target({}) {{",
        (0..150)
            .map(|i| format!("arg{i}: string"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let body = format!(
        "{signature}\n{}\n}}\nexport function start() {{ target(); return '中文🙂'; }}",
        " console.log('中文');\n".repeat(2000)
    );
    let text = explore(&[("large.ts", body)], "start target", 1);
    assert!(
        signature.len() + 300 < 3800,
        "signature must fit with labels"
    );
    assert!(text.contains(&signature), "signature cut");
    assert!(
        text.contains("signature excerpt in ") && text.contains("; body omitted"),
        "{text}"
    );
    assert!(!text.contains("console.log"), "body must remain omitted");
    assert_source_budget(&text);
}

fn assert_source_budget(text: &str) {
    // Check the decoded tool text, rather than JSON escaping overhead.
    assert!(
        text.len() <= 13_000,
        "total output exceeded small-repo budget"
    );
    let source = text.split_once("#### large.ts").unwrap().1;
    let source = source.split("\nUncovered source anchors").next().unwrap();
    assert!(source.len() + "#### large.ts".len() <= 3800);
    assert!(text.contains("中文🙂"), "UTF-8 source evidence lost");
    assert!(!text.contains('\u{fffd}'), "invalid UTF-8 replacement");
}

#[test]
fn case_026_signature_exceeding_budget_reports_gap() {
    let signature = format!(
        "export function target({}) {{",
        (0..500)
            .map(|i| format!("arg{i}: string"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    assert!(signature.len() > 3800);
    let body = format!(
        "{signature}\n{}\n}}\nexport function start() {{ target(); return '中文🙂'; }}",
        " console.log('中文🙂');\n".repeat(2000)
    );
    let text = explore(&[("large.ts", body)], "start target", 1);
    assert!(text.contains("uncovered anchor lines: 1; source omitted"));
    assert!(!text.contains(&signature));
    assert!(!text.contains("signature excerpt"));
    assert!(!text.contains("complete target"));
    assert!(!text.contains("large.ts · full"));
    assert!(!text.contains("1\texport function target"));
    assert_source_budget(&text);
}
