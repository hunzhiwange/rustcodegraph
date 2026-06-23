use crate::common::*;

#[test]
fn should_start_and_stop_without_errors() {
    let _guard = TestGuard::new();
    let project = TempProject::new("codegraph-watcher");
    let (_calls, sync_fn) = sync_mock(Vec::new(), Ok(ok(0, 0)));
    let mut watcher = FileWatcher::new(project.path(), sync_fn, inert_options(2000));

    let started = watcher.start();
    assert!(started);
    assert!(watcher.is_active());

    watcher.stop();
    assert!(!watcher.is_active());
}

#[test]
fn should_be_idempotent_on_double_start() {
    let _guard = TestGuard::new();
    let project = TempProject::new("codegraph-watcher");
    let (_calls, sync_fn) = sync_mock(Vec::new(), Ok(ok(0, 0)));
    let mut watcher = FileWatcher::new(project.path(), sync_fn, inert_options(2000));

    assert!(watcher.start());
    assert!(watcher.start());
    assert!(watcher.is_active());

    watcher.stop();
}

#[test]
fn should_be_idempotent_on_double_stop() {
    let _guard = TestGuard::new();
    let project = TempProject::new("codegraph-watcher");
    let (_calls, sync_fn) = sync_mock(Vec::new(), Ok(ok(0, 0)));
    let mut watcher = FileWatcher::new(project.path(), sync_fn, inert_options(2000));

    watcher.start();
    watcher.stop();
    watcher.stop();

    assert!(!watcher.is_active());
}
