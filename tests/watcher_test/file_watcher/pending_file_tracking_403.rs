use crate::common::*;

#[test]
fn should_expose_edited_paths_via_get_pending_files_before_sync_fires() {
    let _guard = TestGuard::new();
    let project = TempProject::new("codegraph-watcher");
    let (_calls, sync_fn) = sync_mock(Vec::new(), Ok(ok(1, 10)));
    let mut watcher = FileWatcher::new(project.path(), sync_fn, inert_options(2000));
    watcher.start();
    watcher
        .wait_until_ready(1000)
        .expect("watcher should be ready");

    assert_eq!(watcher.get_pending_files(), []);

    assert!(__emit_watch_event_for_tests(
        project.path(),
        "src/pending.ts"
    ));

    let pending = watcher.get_pending_files();
    let paths = pending
        .iter()
        .map(|entry| entry.path.as_str())
        .collect::<Vec<_>>();
    assert!(paths.contains(&"src/pending.ts"));
    let entry = pending
        .iter()
        .find(|entry| entry.path == "src/pending.ts")
        .expect("pending entry should exist");
    assert!(entry.first_seen_ms > 0);
    assert!(entry.last_seen_ms >= entry.first_seen_ms);
    assert!(!entry.indexing);

    watcher.stop();
}

#[test]
fn should_clear_an_entry_only_after_a_successful_sync_absorbing_that_edit() {
    let _guard = TestGuard::new();
    let project = TempProject::new("codegraph-watcher");
    let (calls, sync_fn) = sync_mock(Vec::new(), Ok(ok(1, 10)));
    let mut watcher = FileWatcher::new(project.path(), sync_fn, inert_options(200));
    watcher.start();
    watcher
        .wait_until_ready(1000)
        .expect("watcher should be ready");

    assert!(__emit_watch_event_for_tests(project.path(), "src/fresh.ts"));

    assert!(
        watcher
            .get_pending_files()
            .iter()
            .any(|entry| entry.path == "src/fresh.ts")
    );

    wait_for(
        || {
            watcher.flush_due();
            calls.load(Ordering::SeqCst) > 0
        },
        2000,
        25,
    );
    wait_for(
        || {
            !watcher
                .get_pending_files()
                .iter()
                .any(|entry| entry.path == "src/fresh.ts")
        },
        2000,
        25,
    );

    assert_eq!(watcher.get_pending_files(), []);
    watcher.stop();
}

#[test]
fn should_keep_entries_unchanged_when_sync_fails_rescheduled_work_sees_the_same_set() {
    let _guard = TestGuard::new();
    let project = TempProject::new("codegraph-watcher");
    let (_calls, sync_fn) = sync_mock(
        vec![Err(WatchSyncError::Other("boom".to_owned())), Ok(ok(1, 10))],
        Ok(ok(1, 10)),
    );
    let error_calls = Arc::new(Mutex::new(Vec::<WatchSyncError>::new()));
    let on_error = Arc::clone(&error_calls);
    let mut watcher = FileWatcher::new(
        project.path(),
        sync_fn,
        FileWatchOptions {
            debounce_ms: Some(100),
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

    assert!(__emit_watch_event_for_tests(
        project.path(),
        "src/will-fail.ts"
    ));

    wait_for(
        || {
            watcher.flush_due();
            !error_calls
                .lock()
                .expect("error lock should not be poisoned")
                .is_empty()
        },
        2000,
        20,
    );

    let after = watcher.get_pending_files();
    assert!(after.iter().any(|entry| entry.path == "src/will-fail.ts"));

    wait_for(
        || {
            watcher.flush_due();
            !watcher
                .get_pending_files()
                .iter()
                .any(|entry| entry.path == "src/will-fail.ts")
        },
        2000,
        20,
    );

    watcher.stop();
}

#[test]
fn should_retain_pending_files_and_retry_when_sync_fn_throws_lock_unavailable_error_449() {
    let _guard = TestGuard::new();
    let project = TempProject::new("codegraph-watcher");
    let (calls, sync_fn) = sync_mock(
        vec![Err(LockUnavailableError::default().into()), Ok(ok(1, 10))],
        Ok(ok(1, 10)),
    );
    let complete_calls = Arc::new(Mutex::new(Vec::<SyncRunResult>::new()));
    let on_complete = Arc::clone(&complete_calls);
    let error_calls = Arc::new(Mutex::new(Vec::<WatchSyncError>::new()));
    let on_error = Arc::clone(&error_calls);
    let mut watcher = FileWatcher::new(
        project.path(),
        sync_fn,
        FileWatchOptions {
            debounce_ms: Some(100),
            inert_for_tests: true,
            on_sync_complete: Some(Box::new(move |result| {
                on_complete
                    .lock()
                    .expect("complete lock should not be poisoned")
                    .push(*result);
            })),
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

    assert!(__emit_watch_event_for_tests(
        project.path(),
        "src/locked.ts"
    ));

    wait_for(
        || {
            watcher.flush_due();
            calls.load(Ordering::SeqCst) >= 1
        },
        2000,
        20,
    );
    assert!(
        watcher
            .get_pending_files()
            .iter()
            .any(|entry| entry.path == "src/locked.ts")
    );
    assert!(
        error_calls
            .lock()
            .expect("error lock should not be poisoned")
            .is_empty()
    );
    assert!(
        complete_calls
            .lock()
            .expect("complete lock should not be poisoned")
            .is_empty()
    );

    wait_for(
        || {
            watcher.flush_due();
            calls.load(Ordering::SeqCst) >= 2
        },
        2000,
        20,
    );
    wait_for(
        || {
            !watcher
                .get_pending_files()
                .iter()
                .any(|entry| entry.path == "src/locked.ts")
        },
        2000,
        20,
    );

    assert_eq!(
        *complete_calls
            .lock()
            .expect("complete lock should not be poisoned"),
        vec![ok(1, 10)]
    );
    assert!(
        error_calls
            .lock()
            .expect("error lock should not be poisoned")
            .is_empty()
    );

    watcher.stop();
}
