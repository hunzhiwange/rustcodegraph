use crate::common::*;

#[test]
fn fails_to_start_and_degrades_when_fs_watch_setup_exhausts_watch_resources() {
    let _guard = TestGuard::new();
    let project = TempProject::empty("codegraph-exhaust");
    let degraded = Arc::new(Mutex::new(Vec::<String>::new()));
    let on_degraded = Arc::clone(&degraded);
    let warnings = capture_warnings();

    __set_fs_watch_for_tests(Some(Arc::new(|_dir| {
        Err(WatchStartError::resource_exhaustion("too many open files"))
    })));

    let (_calls, sync_fn) = sync_mock(Vec::new(), Ok(ok(0, 0)));
    let mut watcher = FileWatcher::new(
        project.path(),
        sync_fn,
        FileWatchOptions {
            debounce_ms: Some(100),
            on_degraded: Some(Box::new(move |reason| {
                on_degraded
                    .lock()
                    .expect("degraded lock should not be poisoned")
                    .push(reason.to_owned());
            })),
            ..FileWatchOptions::default()
        },
    );

    assert!(!watcher.start());
    assert!(!watcher.is_active());
    assert!(watcher.is_degraded());
    assert!(
        watcher
            .get_degraded_reason()
            .expect("degraded reason should be set")
            .contains("auto-sync disabled")
    );
    let degraded = degraded
        .lock()
        .expect("degraded lock should not be poisoned");
    assert_eq!(degraded.len(), 1);
    assert!(degraded[0].contains("auto-sync disabled"));
    assert_eq!(warning_count(&warnings, "File watcher disabled"), 1);
}

#[test]
fn degrades_exactly_once_when_the_live_watcher_emits_emfile_at_runtime() {
    let _guard = TestGuard::new();
    let project = TempProject::empty("codegraph-exhaust");
    let degraded = Arc::new(Mutex::new(Vec::<String>::new()));
    let on_degraded = Arc::clone(&degraded);
    let warnings = capture_warnings();
    let closed = Arc::new(AtomicUsize::new(0));
    let error_handler = Arc::new(Mutex::new(None));
    let factory_closed = Arc::clone(&closed);
    let factory_handler = Arc::clone(&error_handler);
    __set_fs_watch_for_tests(Some(Arc::new(move |_dir| {
        Ok(Box::new(CountingWatchHandle {
            closed: Arc::clone(&factory_closed),
            error_handler: Arc::clone(&factory_handler),
        }))
    })));

    let (_calls, sync_fn) = sync_mock(Vec::new(), Ok(ok(0, 0)));
    let mut watcher = FileWatcher::new(
        project.path(),
        sync_fn,
        FileWatchOptions {
            debounce_ms: Some(100),
            on_degraded: Some(Box::new(move |reason| {
                on_degraded
                    .lock()
                    .expect("degraded lock should not be poisoned")
                    .push(reason.to_owned());
            })),
            ..FileWatchOptions::default()
        },
    );

    assert!(watcher.start());
    assert!(watcher.is_active());

    let handler = error_handler
        .lock()
        .expect("error handler lock should not be poisoned");
    let handler = handler
        .as_ref()
        .expect("watch error handler should be installed");
    handler(WatchStartError::resource_exhaustion("too many open files"));
    handler(WatchStartError::resource_exhaustion("too many open files"));

    assert!(!watcher.is_active());
    assert!(watcher.is_degraded());
    assert_eq!(
        degraded
            .lock()
            .expect("degraded lock should not be poisoned")
            .len(),
        1
    );
    assert_eq!(closed.load(Ordering::SeqCst), 1);
    assert_eq!(warning_count(&warnings, "File watcher disabled"), 1);
}

#[test]
fn reports_is_degraded_false_null_reason_while_healthy() {
    let _guard = TestGuard::new();
    let project = TempProject::new("codegraph-watcher");
    let (_calls, sync_fn) = sync_mock(Vec::new(), Ok(ok(0, 0)));
    let mut watcher = FileWatcher::new(project.path(), sync_fn, inert_options(2000));
    watcher.start();

    assert!(!watcher.is_degraded());
    assert!(watcher.get_degraded_reason().is_none());

    watcher.stop();
}

#[test]
fn warns_once_not_degrade_when_linux_inotify_watches_are_exhausted_enospc() {
    let _guard = TestGuard::new();
    __set_supports_recursive_watch_for_tests(Some(false));
    let project = TempProject::empty("codegraph-inotify");
    fs::create_dir(project.path().join("sub")).expect("subdir should be created");
    let degraded = Arc::new(Mutex::new(Vec::<String>::new()));
    let on_degraded = Arc::clone(&degraded);
    let warnings = capture_warnings();
    let calls = Arc::new(AtomicUsize::new(0));
    let factory_calls = Arc::clone(&calls);
    __set_fs_watch_for_tests(Some(Arc::new(move |_dir| {
        let call = factory_calls.fetch_add(1, Ordering::SeqCst) + 1;
        if call == 1 {
            Ok(Box::new(CountingWatchHandle {
                closed: Arc::new(AtomicUsize::new(0)),
                error_handler: Arc::new(Mutex::new(None)),
            }))
        } else {
            Err(WatchStartError::inotify_exhaustion(
                "ENOSPC: System limit for number of file watchers reached",
            ))
        }
    })));

    let (_calls, sync_fn) = sync_mock(Vec::new(), Ok(ok(0, 0)));
    let mut watcher = FileWatcher::new(
        project.path(),
        sync_fn,
        FileWatchOptions {
            debounce_ms: Some(100),
            on_degraded: Some(Box::new(move |reason| {
                on_degraded
                    .lock()
                    .expect("degraded lock should not be poisoned")
                    .push(reason.to_owned());
            })),
            ..FileWatchOptions::default()
        },
    );

    assert!(watcher.start());
    assert!(watcher.is_active());
    assert!(!watcher.is_degraded());
    assert!(
        degraded
            .lock()
            .expect("degraded lock should not be poisoned")
            .is_empty()
    );
    let warning = warning_with(&warnings, "inotify watch limit")
        .expect("inotify warning should be logged exactly once");
    assert!(warning.contains("fs.inotify.max_user_watches"));
    assert_eq!(warning_count(&warnings, "inotify watch limit"), 1);

    watcher.stop();
}
