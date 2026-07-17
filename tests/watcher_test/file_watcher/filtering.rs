use crate::common::*;

struct QueuedWatchHandle {
    events: Arc<Mutex<Vec<std::path::PathBuf>>>,
}

impl WatchHandle for QueuedWatchHandle {
    fn close(&mut self) {}

    fn take_events(&mut self) -> Vec<std::path::PathBuf> {
        let mut events = self.events.lock().expect("events lock should not poison");
        std::mem::take(&mut *events)
    }
}

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
fn should_trigger_sync_for_deleted_directory_path_events() {
    let _guard = TestGuard::new();
    __set_supports_recursive_watch_for_tests(Some(true));
    let project = TempProject::new("codegraph-watcher");
    let deleted_dir = project.path().join("src").join("deleted-feature");
    fs::create_dir_all(&deleted_dir).expect("deleted-feature directory should be created");
    fs::write(
        deleted_dir.join("old.ts"),
        "export function removedFeature() { return 1; }\n",
    )
    .expect("source file should be written");

    let events = Arc::new(Mutex::new(Vec::<std::path::PathBuf>::new()));
    let factory_events = Arc::clone(&events);
    __set_fs_watch_for_tests(Some(Arc::new(move |_dir| {
        Ok(Box::new(QueuedWatchHandle {
            events: Arc::clone(&factory_events),
        }))
    })));

    let (calls, sync_fn) = sync_mock(Vec::new(), Ok(ok(1, 10)));
    let mut watcher = FileWatcher::new(
        project.path(),
        sync_fn,
        FileWatchOptions {
            debounce_ms: Some(50),
            ..FileWatchOptions::default()
        },
    );

    assert!(watcher.start());
    watcher
        .wait_until_ready(1000)
        .expect("watcher should be ready");

    fs::remove_dir_all(&deleted_dir).expect("deleted-feature directory should be removed");
    events
        .lock()
        .expect("events lock should not poison")
        .push(deleted_dir);

    wait_for(
        || {
            watcher.tick();
            calls.load(Ordering::SeqCst) > 0
        },
        2000,
        25,
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    watcher.stop();
}

#[test]
fn recursive_directory_events_do_not_expand_per_directory_watchers() {
    let _guard = TestGuard::new();
    __set_supports_recursive_watch_for_tests(Some(true));
    let project = TempProject::new("codegraph-recursive-watcher");
    let events = Arc::new(Mutex::new(Vec::<std::path::PathBuf>::new()));
    let watch_creations = Arc::new(AtomicUsize::new(0));
    let factory_events = Arc::clone(&events);
    let factory_watch_creations = Arc::clone(&watch_creations);
    __set_fs_watch_for_tests(Some(Arc::new(move |_dir| {
        factory_watch_creations.fetch_add(1, Ordering::SeqCst);
        Ok(Box::new(QueuedWatchHandle {
            events: Arc::clone(&factory_events),
        }))
    })));

    let (calls, sync_fn) = sync_mock(Vec::new(), Ok(ok(1, 10)));
    let mut watcher = FileWatcher::new(
        project.path(),
        sync_fn,
        FileWatchOptions {
            debounce_ms: Some(50),
            ..FileWatchOptions::default()
        },
    );

    assert!(watcher.start());
    watcher
        .wait_until_ready(1000)
        .expect("watcher should be ready");
    assert_eq!(watch_creations.load(Ordering::SeqCst), 1);

    let copied_dir = project.path().join("src").join("copied-feature");
    let nested_dir = copied_dir.join("nested");
    fs::create_dir_all(&nested_dir).expect("copied feature directory should be created");
    fs::write(
        nested_dir.join("feature.ts"),
        "export function copiedFeature() { return 1; }\n",
    )
    .expect("copied source file should be written");
    events
        .lock()
        .expect("events lock should not poison")
        .push(copied_dir);

    watcher.tick();
    assert_eq!(
        watch_creations.load(Ordering::SeqCst),
        1,
        "a recursive watcher must not add one OS watcher per directory"
    );

    thread::sleep(Duration::from_millis(75));
    watcher.flush_due();
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    watcher.stop();
}

#[test]
fn per_directory_events_still_attach_watchers_for_new_subtrees() {
    let _guard = TestGuard::new();
    __set_supports_recursive_watch_for_tests(Some(false));
    let project = TempProject::new("codegraph-per-directory-watcher");
    let events = Arc::new(Mutex::new(Vec::<std::path::PathBuf>::new()));
    let watch_creations = Arc::new(AtomicUsize::new(0));
    let factory_events = Arc::clone(&events);
    let factory_watch_creations = Arc::clone(&watch_creations);
    __set_fs_watch_for_tests(Some(Arc::new(move |_dir| {
        factory_watch_creations.fetch_add(1, Ordering::SeqCst);
        Ok(Box::new(QueuedWatchHandle {
            events: Arc::clone(&factory_events),
        }))
    })));

    let (calls, sync_fn) = sync_mock(Vec::new(), Ok(ok(1, 10)));
    let mut watcher = FileWatcher::new(
        project.path(),
        sync_fn,
        FileWatchOptions {
            debounce_ms: Some(50),
            ..FileWatchOptions::default()
        },
    );

    assert!(watcher.start());
    watcher
        .wait_until_ready(1000)
        .expect("watcher should be ready");
    assert_eq!(watch_creations.load(Ordering::SeqCst), 2);

    let copied_dir = project.path().join("src").join("copied-feature");
    let nested_dir = copied_dir.join("nested");
    fs::create_dir_all(&nested_dir).expect("copied feature directory should be created");
    fs::write(
        nested_dir.join("feature.ts"),
        "export function copiedFeature() { return 1; }\n",
    )
    .expect("copied source file should be written");
    events
        .lock()
        .expect("events lock should not poison")
        .push(copied_dir);

    watcher.tick();
    assert_eq!(
        watch_creations.load(Ordering::SeqCst),
        4,
        "per-directory mode must attach watchers to both new directories"
    );

    thread::sleep(Duration::from_millis(75));
    watcher.flush_due();
    assert_eq!(calls.load(Ordering::SeqCst), 1);

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
