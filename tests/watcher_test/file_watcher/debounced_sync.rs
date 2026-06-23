use crate::common::*;

#[test]
fn should_trigger_sync_after_file_change() {
    let _guard = TestGuard::new();
    let project = TempProject::new("codegraph-watcher");
    let (calls, sync_fn) = sync_mock(Vec::new(), Ok(ok(1, 10)));
    let mut watcher = FileWatcher::new(project.path(), sync_fn, inert_options(200));

    watcher.start();
    watcher
        .wait_until_ready(1000)
        .expect("watcher should be ready");
    assert!(__emit_watch_event_for_tests(project.path(), "src/new.ts"));

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

#[test]
fn should_debounce_rapid_changes_into_a_single_sync() {
    let _guard = TestGuard::new();
    let project = TempProject::new("codegraph-watcher");
    let (calls, sync_fn) = sync_mock(Vec::new(), Ok(ok(1, 10)));
    let mut watcher = FileWatcher::new(project.path(), sync_fn, inert_options(400));

    watcher.start();
    watcher
        .wait_until_ready(1000)
        .expect("watcher should be ready");

    for i in 0..5 {
        assert!(__emit_watch_event_for_tests(
            project.path(),
            format!("src/file{i}.ts")
        ));
        thread::sleep(Duration::from_millis(50));
        watcher.flush_due();
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }

    wait_for(
        || {
            watcher.flush_due();
            calls.load(Ordering::SeqCst) > 0
        },
        2000,
        25,
    );

    assert_eq!(calls.load(Ordering::SeqCst), 1);

    watcher.stop();
}
