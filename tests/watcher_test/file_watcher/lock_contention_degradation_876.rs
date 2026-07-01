use crate::common::*;

#[test]
fn disables_auto_sync_after_prolonged_lock_contention_with_bounded_retries() {
    let _guard = TestGuard::new();
    let project = TempProject::new("codegraph-watcher");
    let (calls, sync_fn) = sync_mock(Vec::new(), Err(LockUnavailableError::default().into()));
    let complete_calls = Arc::new(Mutex::new(Vec::<SyncRunResult>::new()));
    let on_complete = Arc::clone(&complete_calls);
    let error_calls = Arc::new(Mutex::new(Vec::<WatchSyncError>::new()));
    let on_error = Arc::clone(&error_calls);
    let degraded = Arc::new(Mutex::new(Vec::<String>::new()));
    let on_degraded = Arc::clone(&degraded);
    let warnings = capture_warnings();
    let mut watcher = FileWatcher::new(
        project.path(),
        sync_fn,
        FileWatchOptions {
            debounce_ms: Some(25),
            min_sync_interval_ms: None,
            max_debounce_ms: None,
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
            on_degraded: Some(Box::new(move |reason| {
                on_degraded
                    .lock()
                    .expect("degraded lock should not be poisoned")
                    .push(reason.to_owned());
            })),
        },
    );

    watcher.start();
    watcher
        .wait_until_ready(1000)
        .expect("watcher should be ready");
    assert!(__emit_watch_event_for_tests(
        project.path(),
        "src/long-lock.ts"
    ));

    while watcher.is_active() {
        watcher.flush_now();
    }

    assert!(calls.load(Ordering::SeqCst) >= 6);
    assert!(watcher.is_degraded());
    let degraded = degraded
        .lock()
        .expect("degraded lock should not be poisoned");
    assert_eq!(degraded.len(), 1);
    assert!(degraded[0].contains("auto-sync disabled"));
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
    assert_eq!(watcher.get_pending_files(), []);
    assert_eq!(warning_count(&warnings, "File watcher disabled"), 1);
}

#[test]
fn does_not_degrade_on_brief_contention_backoff_resets_after_a_clean_sync() {
    let _guard = TestGuard::new();
    let project = TempProject::new("codegraph-watcher");
    let (calls, sync_fn) = sync_mock(
        vec![
            Err(LockUnavailableError::default().into()),
            Err(LockUnavailableError::default().into()),
            Err(LockUnavailableError::default().into()),
            Ok(ok(1, 5)),
        ],
        Ok(ok(1, 5)),
    );
    let complete_calls = Arc::new(Mutex::new(Vec::<SyncRunResult>::new()));
    let on_complete = Arc::clone(&complete_calls);
    let degraded = Arc::new(Mutex::new(Vec::<String>::new()));
    let on_degraded = Arc::clone(&degraded);
    let mut watcher = FileWatcher::new(
        project.path(),
        sync_fn,
        FileWatchOptions {
            debounce_ms: Some(25),
            inert_for_tests: true,
            on_sync_complete: Some(Box::new(move |result| {
                on_complete
                    .lock()
                    .expect("complete lock should not be poisoned")
                    .push(*result);
            })),
            on_degraded: Some(Box::new(move |reason| {
                on_degraded
                    .lock()
                    .expect("degraded lock should not be poisoned")
                    .push(reason.to_owned());
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
        "src/brief-lock.ts"
    ));

    wait_for(
        || {
            watcher.flush_now();
            !complete_calls
                .lock()
                .expect("complete lock should not be poisoned")
                .is_empty()
        },
        4000,
        20,
    );

    assert_eq!(calls.load(Ordering::SeqCst), 4);
    assert!(
        degraded
            .lock()
            .expect("degraded lock should not be poisoned")
            .is_empty()
    );
    assert!(!watcher.is_degraded());
    assert!(watcher.is_active());
    assert!(
        !watcher
            .get_pending_files()
            .iter()
            .any(|entry| entry.path == "src/brief-lock.ts")
    );

    watcher.stop();
}
