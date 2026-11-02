use rustcodegraph::mcp::tools::ToolHandler;
use rustcodegraph::{CodeGraph, IndexOptions};
use serde_json::json;
fn run(repeat: bool, edit: bool) -> (String, String) {
    let root = std::env::temp_dir().join(format!(
        "rcg30-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    for name in ["alpha", "beta"] {
        let body = format!(
            "export function {name}() {{\n{}\n}}",
            (0..200)
                .map(|i| format!(" console.log('{name}_{i} padding padding padding');"))
                .collect::<Vec<_>>()
                .join("\n")
        );
        std::fs::write(root.join(format!("{name}.ts")), body).unwrap();
    }
    let mut cg = CodeGraph::init_sync(&root).unwrap();
    assert!(cg.index_all(IndexOptions::default()).success);
    let mut h = ToolHandler::new(true);
    h.set_default_code_graph(&cg);
    let mut state = rustcodegraph::mcp::explore_session_state::ExploreSessionState::default();
    let args = json!({"query":"alpha"});
    h.execute_in_session(
        "rustcodegraph_explore",
        args.as_object().unwrap(),
        &mut state,
    );
    if edit {
        let p = root.join("alpha.ts");
        let mut s = std::fs::read_to_string(&p).unwrap();
        s.push_str("\n// changed");
        std::fs::write(p, s).unwrap();
    }
    let args = json!({"query":if repeat {"alpha"} else {"alpha beta"}});
    let warm = h
        .execute_in_session(
            "rustcodegraph_explore",
            args.as_object().unwrap(),
            &mut state,
        )
        .content
        .into_iter()
        .map(|c| c.text)
        .collect::<String>();
    let cold = h
        .execute("rustcodegraph_explore", args.as_object().unwrap())
        .content
        .into_iter()
        .map(|c| c.text)
        .collect::<String>();
    h.close_all();
    cg.destroy();
    std::fs::remove_dir_all(root).unwrap();
    (warm, cold)
}
#[test]
fn case_030_reclaims_for_new_source() {
    let (warm, cold) = run(false, false);
    assert!(
        warm.matches("beta_").count() > cold.matches("beta_").count(),
        "warm={} cold={}",
        warm.matches("beta_").count(),
        cold.matches("beta_").count()
    );
}
#[test]
fn case_030_all_repeat_resends_source() {
    let (warm, _) = run(true, false);
    assert!(warm.contains("1\texport function alpha"));
}
#[test]
fn case_030_changed_fingerprint_resends() {
    let (warm, _) = run(true, true);
    assert!(!warm.contains("already sent in this session"));
    assert!(warm.contains("alpha_0"));
}
