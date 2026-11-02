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
fn case_029_pointer_and_boundaries() {
    let (dir, mut cg) = fixture();
    let source = (1..=12).map(|n| format!("line{n}\n")).collect::<String>();
    std::fs::write(dir.path().join("flow.ts"), &source).unwrap();
    let response = format!(
        "#### flow.ts · full\n{}",
        (1..=12)
            .map(|n| format!("{n}\tline{n}\n"))
            .collect::<String>()
    );
    let mut state = ExploreSessionState::default();
    assert_eq!(state.deduplicate(dir.path(), &response), response);
    state.record_response(dir.path(), &response);
    assert!(
        state
            .deduplicate(dir.path(), &response)
            .contains("already sent in this session")
    );
    let short = "#### flow.ts · full\n1\tline1\n";
    assert_eq!(state.deduplicate(dir.path(), short), short);
    std::fs::write(dir.path().join("flow.ts"), format!("{source}// edit")).unwrap();
    assert_eq!(state.deduplicate(dir.path(), &response), response);
    cg.close();
}
