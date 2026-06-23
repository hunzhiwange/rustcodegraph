use crate::common::*;

#[test]
fn should_ignore_files_not_matching_include_patterns() {
    let _guard = TestGuard::new();
    let project = TempProject::new("codegraph-watcher");
    let (calls, sync_fn) = sync_mock(Vec::new(), Ok(ok(0, 0)));
    let mut watcher = FileWatcher::new(project.path(), sync_fn, inert_options(200));

    watcher.start();
    watcher
        .wait_until_ready(1000)
        .expect("watcher should be ready");
    assert!(__emit_watch_event_for_tests(
        project.path(),
        "src/readme.md"
    ));

    thread::sleep(Duration::from_millis(400));
    watcher.flush_due();
    assert_eq!(calls.load(Ordering::SeqCst), 0);

    watcher.stop();
}

#[test]
fn should_ignore_codegraph_directory_changes() {
    let _guard = TestGuard::new();
    let project = TempProject::new("codegraph-watcher");
    let (calls, sync_fn) = sync_mock(Vec::new(), Ok(ok(0, 0)));
    let mut watcher = FileWatcher::new(project.path(), sync_fn, inert_options(200));

    watcher.start();
    watcher
        .wait_until_ready(1000)
        .expect("watcher should be ready");
    assert!(__emit_watch_event_for_tests(
        project.path(),
        ".rustcodegraph/db.sqlite"
    ));

    thread::sleep(Duration::from_millis(400));
    watcher.flush_due();
    assert_eq!(calls.load(Ordering::SeqCst), 0);

    watcher.stop();
}

#[test]
fn should_drop_ignored_non_source_paths_but_sync_real_source_edits() {
    let _guard = TestGuard::new();
    let project = TempProject::new("codegraph-watcher");
    let (calls, sync_fn) = sync_mock(Vec::new(), Ok(ok(0, 0)));
    let mut watcher = FileWatcher::new(project.path(), sync_fn, inert_options(200));
    watcher.start();
    watcher
        .wait_until_ready(1000)
        .expect("watcher should be ready");

    assert!(__emit_watch_event_for_tests(
        project.path(),
        "node_modules/dep/index.js"
    ));
    assert!(__emit_watch_event_for_tests(project.path(), "src/live.ts"));
    wait_for(
        || {
            watcher.flush_due();
            calls.load(Ordering::SeqCst) > 0
        },
        2000,
        25,
    );
    assert!(calls.load(Ordering::SeqCst) > 0);

    watcher.stop();
}
