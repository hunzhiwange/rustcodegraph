use crate::common::*;

#[test]
fn should_watch_and_unwatch_via_code_graph_api() {
    let _guard = TestGuard::new();
    let project = TempProject::new("codegraph-watcher");
    let mut cg = CodeGraph::init_sync(project.path()).expect("CodeGraph init should work");
    let _ = cg.index_all(IndexOptions::default());

    assert!(!cg.is_watching());

    let started = cg.watch(CodeGraphWatchOptions {
        debounce_ms: Some(200),
    });
    assert!(started);
    assert!(cg.is_watching());

    cg.unwatch();
    assert!(!cg.is_watching());
}

#[test]
fn should_stop_watching_on_close() {
    let _guard = TestGuard::new();
    let project = TempProject::new("codegraph-watcher");
    let mut cg = CodeGraph::init_sync(project.path()).expect("CodeGraph init should work");
    let _ = cg.index_all(IndexOptions::default());

    cg.watch(CodeGraphWatchOptions {
        debounce_ms: Some(200),
    });
    assert!(cg.is_watching());

    cg.close();
    assert!(!cg.is_watching());
}

#[test]
fn should_auto_sync_when_files_change_while_watching_real_fs_watch_end_to_end() {
    let _guard = TestGuard::new();
    let project = TempProject::new("codegraph-watcher");
    let mut cg = CodeGraph::init_sync(project.path()).expect("CodeGraph init should work");
    let _ = cg.index_all(IndexOptions::default());
    let initial_stats = cg.get_stats();
    let initial_nodes = initial_stats.node_count;

    cg.watch(CodeGraphWatchOptions {
        debounce_ms: Some(300),
    });
    thread::sleep(Duration::from_millis(100));

    fs::write(
        project.path().join("src").join("added.ts"),
        "export function added() { return 42; }",
    )
    .expect("added source file should be written");

    wait_for(
        || {
            let stats = cg.get_stats();
            stats.node_count > initial_nodes
        },
        8000,
        25,
    );

    let results = cg.search_nodes("added", None);
    assert!(!results.is_empty());

    cg.unwatch();
}
