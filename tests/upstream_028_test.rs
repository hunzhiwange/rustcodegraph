use rustcodegraph::mcp::explore_session_state::ExploreSessionState;
use rustcodegraph::{CodeGraph, IndexOptions};
struct Dir(std::path::PathBuf);
impl Dir {
    fn path(&self) -> &std::path::Path {
        &self.0
    }
}
impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn fixture() -> (Dir, CodeGraph) {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let dir = Dir(std::env::temp_dir().join(format!(
        "rcg28-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    )));
    std::fs::create_dir_all(dir.path()).unwrap();
    std::fs::write(
        dir.path().join("flow.ts"),
        "export function start() {\n  return 42;\n}\n",
    )
    .unwrap();
    let mut cg = CodeGraph::init_sync(dir.path()).unwrap();
    assert!(cg.index_all(IndexOptions::default()).success);
    (dir, cg)
}
#[test]
fn case_028_records_only_verified_complete_lines() {
    let (dir, mut cg) = fixture();
    let mut state = ExploreSessionState::default();
    state.record_response(
        dir.path(),
        "#### flow.ts · full\n1\texport function start() {\n2\t  return",
    );
    assert_eq!(state.projects[0].calls[0].files[0].ranges, vec![(1, 1)]);
    cg.close();
}
#[test]
fn case_028_sessions_and_projects_are_isolated() {
    let (a, mut ca) = fixture();
    let (b, mut cb) = fixture();
    let mut state = ExploreSessionState::default();
    state.record_response(a.path(), "");
    state.record_response(b.path(), "");
    state.record_response(&a.path().join("."), "");
    assert_eq!(state.projects.len(), 2);
    assert_eq!(state.projects.back().unwrap().call_count, 2);
    assert!(ExploreSessionState::default().projects.is_empty());
    ca.close();
    cb.close();
}
#[test]
fn case_028_bounds_detail_without_resetting_counters() {
    let (dir, mut cg) = fixture();
    let mut state = ExploreSessionState::default();
    for _ in 0..30 {
        state.record_response(
            dir.path(),
            "#### flow.ts · full\n1\texport function start() {",
        );
    }
    assert_eq!(state.projects[0].call_count, 30);
    assert_eq!(state.projects[0].calls.len(), 8);
    assert!(state.projects[0].response_bytes > state.projects[0].calls[0].response_bytes);
    cg.close();
}
#[test]
fn case_028_handler_records_real_explore_and_changed_fingerprint() {
    let (dir, mut cg) = fixture();
    let mut handler = rustcodegraph::mcp::tools::ToolHandler::new(true);
    handler.set_default_code_graph(&cg);
    let mut state = ExploreSessionState::default();
    let args = serde_json::json!({"query":"start"});
    let result = handler.execute_in_session(
        "rustcodegraph_explore",
        args.as_object().unwrap(),
        &mut state,
    );
    assert_ne!(result.is_error, Some(true));
    assert_eq!(state.projects[0].call_count, 1);
    assert!(!state.projects[0].calls[0].files.is_empty());
    let fingerprint = state.projects[0].calls[0].files[0].fingerprint;
    std::fs::write(
        dir.path().join("flow.ts"),
        "export function start() {\n  return 43;\n}\n",
    )
    .unwrap();
    state.record_response(dir.path(), "#### flow.ts · full\n2\t  return 43;");
    assert_ne!(fingerprint, state.projects[0].calls[1].files[0].fingerprint);
    handler.close_all();
    cg.close();
}
#[test]
fn case_028_project_lru_and_missing_root_are_bounded() {
    let mut state = ExploreSessionState::default();
    let mut fixtures = Vec::new();
    for _ in 0..6 {
        let (dir, mut cg) = fixture();
        cg.close();
        state.record_response(dir.path(), "");
        fixtures.push(dir);
    }
    assert_eq!(state.projects.len(), 4);
    assert_eq!(
        state.projects[0].root,
        fixtures[2].path().canonicalize().unwrap()
    );
    state.record_response(&fixtures[0].path().join("missing"), "");
    assert_eq!(state.projects.len(), 4);
}
#[test]
fn case_028_file_and_range_caps_drop_detail() {
    let (dir, mut cg) = fixture();
    let source = (0..100).map(|i| format!("line{i}\n")).collect::<String>();
    let mut response = String::new();
    for i in 0..30 {
        let path = format!("file{i}.txt");
        std::fs::write(dir.path().join(&path), &source).unwrap();
        response.push_str(&format!("#### {path} · full\n"));
        for line in (1..100).step_by(2) {
            response.push_str(&format!("{line}\tline{}\n", line - 1));
        }
    }
    let mut state = ExploreSessionState::default();
    state.record_response(dir.path(), &response);
    let files = &state.projects[0].calls[0].files;
    assert_eq!(files.len(), 24);
    assert!(files.iter().all(|f| f.ranges.len() == 24));
    cg.close();
}
