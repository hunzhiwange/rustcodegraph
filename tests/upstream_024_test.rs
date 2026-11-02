use rustcodegraph::mcp::tools::ToolHandler;
use rustcodegraph::{CodeGraph, IndexOptions};
use serde_json::{Map, Value, json};

fn explore(query: &str, max_files: usize) -> String {
    let root = std::env::temp_dir().join(format!(
        "rcg24-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let dir = root.as_path();
    std::fs::write(
        dir.join("a.ts"),
        format!(
            "export function peripheral() {{ return 1; }}\n{}",
            "// peripheral padding\n".repeat(400)
        ),
    )
    .unwrap();
    std::fs::write(dir.join("z.ts"), format!("export function important() {{ return helper(); }}\nexport function helper() {{ return 2; }}\n{}", "// important padding\n".repeat(600))).unwrap();
    let mut cg = CodeGraph::init_sync(dir).unwrap();
    assert!(cg.index_all(IndexOptions::default()).success);
    let mut handler = ToolHandler::new(true);
    handler.set_default_code_graph(&cg);
    let args: Map<String, Value> =
        serde_json::from_value(json!({"query":query,"maxFiles":max_files})).unwrap();
    let result = handler.execute("rustcodegraph_explore", &args);
    let text = serde_json::to_value(result).unwrap().to_string();
    handler.close_all();
    cg.destroy();
    std::fs::remove_dir_all(dir).unwrap();
    text
}
#[test]
fn case_024_relevance_beats_path_and_length() {
    let text = explore("peripheral important helper", 1);
    assert!(text.contains("#### z.ts"), "{text}");
    assert!(!text.contains("#### a.ts"));
}
#[test]
fn case_024_no_matches_is_recoverable() {
    assert!(explore("absent", 2).contains("No results"));
}
#[test]
fn case_024_reserves_both_files() {
    let text = explore("peripheral important helper", 2);
    assert!(text.contains("#### a.ts") && text.contains("#### z.ts"));
    assert!(text.len() < 18000);
}

#[path = "../src/mcp/tools/budget.rs"]
#[allow(dead_code)]
mod budget;
#[test]
fn case_024_allocation_is_weighted_and_bounded() {
    let shares = budget::allocate_explore_source_budget(&[1, 8], 9000);
    assert!(shares[1] > shares[0]);
    assert!(shares.iter().sum::<usize>() <= 9000);
    assert!(budget::allocate_explore_source_budget(&[], 9000).is_empty());
    assert_eq!(
        budget::allocate_explore_source_budget(&[0, 0], 0),
        vec![0, 0]
    );
    for count in [0, 149, 150, 499, 500, 4999, 5000, 15000, 25000] {
        let a = budget::get_explore_output_budget(count);
        let b = budget::get_explore_output_budget(count + 1);
        assert!(b.max_chars_per_file >= a.max_chars_per_file);
    }
}
