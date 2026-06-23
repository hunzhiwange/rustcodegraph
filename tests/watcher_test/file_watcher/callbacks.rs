use crate::common::*;

#[test]
fn should_call_on_sync_complete_after_successful_sync() {
    let _guard = TestGuard::new();
    let project = TempProject::new("codegraph-watcher");
    let (_calls, sync_fn) = sync_mock(Vec::new(), Ok(ok(2, 50)));
    let complete_calls = Arc::new(Mutex::new(Vec::<SyncRunResult>::new()));
    let on_complete = Arc::clone(&complete_calls);
    let mut watcher = FileWatcher::new(
        project.path(),
        sync_fn,
        FileWatchOptions {
            debounce_ms: Some(200),
            inert_for_tests: true,
            on_sync_complete: Some(Box::new(move |result| {
                on_complete
                    .lock()
                    .expect("complete lock should not be poisoned")
                    .push(*result);
            })),
            ..FileWatchOptions::default()
        },
    );

    watcher.start();
    watcher
        .wait_until_ready(1000)
        .expect("watcher should be ready");
    assert!(__emit_watch_event_for_tests(project.path(), "src/test.ts"));

    wait_for(
        || {
            watcher.flush_due();
            !complete_calls
                .lock()
                .expect("complete lock should not be poisoned")
                .is_empty()
        },
        2000,
        25,
    );
    assert_eq!(
        *complete_calls
            .lock()
            .expect("complete lock should not be poisoned"),
        vec![ok(2, 50)]
    );

    watcher.stop();
}

#[test]
fn should_call_on_sync_error_when_sync_throws() {
    let _guard = TestGuard::new();
    let project = TempProject::new("codegraph-watcher");
    let (_calls, sync_fn) = sync_mock(
        Vec::new(),
        Err(WatchSyncError::Other("sync failed".to_owned())),
    );
    let error_calls = Arc::new(Mutex::new(Vec::<WatchSyncError>::new()));
    let on_error = Arc::clone(&error_calls);
    let mut watcher = FileWatcher::new(
        project.path(),
        sync_fn,
        FileWatchOptions {
            debounce_ms: Some(200),
            inert_for_tests: true,
            on_sync_error: Some(Box::new(move |error| {
                on_error
                    .lock()
                    .expect("error lock should not be poisoned")
                    .push(error.clone());
            })),
            ..FileWatchOptions::default()
        },
    );

    watcher.start();
    watcher
        .wait_until_ready(1000)
        .expect("watcher should be ready");
    assert!(__emit_watch_event_for_tests(project.path(), "src/test.ts"));

    wait_for(
        || {
            watcher.flush_due();
            !error_calls
                .lock()
                .expect("error lock should not be poisoned")
                .is_empty()
        },
        2000,
        25,
    );
    let errors = error_calls
        .lock()
        .expect("error lock should not be poisoned");
    assert!(!errors.is_empty());
    assert!(matches!(errors[0], WatchSyncError::Other(_)));

    watcher.stop();
}
