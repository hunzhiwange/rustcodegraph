use rustcodegraph::mcp::{explore_session_state::ExploreSessionState, tools::ToolHandler};
use rustcodegraph::{CodeGraph, IndexOptions};
use serde_json::json;
#[test]
fn case_031_warning_is_session_local_and_only_for_old_indexes() {
    let root = std::env::temp_dir().join(format!("rcg31-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("a.rs"), "fn hello() {} ").unwrap();
    let mut cg = CodeGraph::init_sync(&root).unwrap();
    assert!(cg.index_all(IndexOptions::default()).success);
    let mut h = ToolHandler::new(true);
    h.set_default_code_graph(&cg);
    let mut state = ExploreSessionState::default();
    let args = json!({});
    let query = |h: &mut ToolHandler, s: &mut ExploreSessionState| {
        h.execute_in_session("rustcodegraph_status", args.as_object().unwrap(), s)
            .content
            .into_iter()
            .map(|c| c.text)
            .collect::<String>()
    };
    assert!(!query(&mut h, &mut state).contains("Refresh notice"));
    let db = rusqlite::Connection::open(root.join(".rustcodegraph/rustcodegraph.db")).unwrap();
    db.execute(
        "UPDATE project_metadata SET value='0' WHERE key='indexed_with_extraction_version'",
        [],
    )
    .unwrap();
    let bad = json!({});
    assert_eq!(
        h.execute_in_session("unknown", bad.as_object().unwrap(), &mut state)
            .is_error,
        Some(true)
    );
    assert!(query(&mut h, &mut state).contains("Refresh notice"));
    assert!(!query(&mut h, &mut state).contains("Refresh notice"));
    assert!(query(&mut h, &mut ExploreSessionState::default()).contains("Refresh notice"));
    drop(db);
    h.close_all();
    cg.destroy();
    std::fs::remove_dir_all(root).unwrap();
}
