use std::collections::VecDeque;
use std::env;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::{MutexGuard, OnceLock};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

pub(crate) use std::fs;
pub(crate) use std::sync::atomic::{AtomicUsize, Ordering};
pub(crate) use std::sync::{Arc, Mutex};
pub(crate) use std::thread;
pub(crate) use std::time::Duration;

use rustcodegraph::errors::{DEFAULT_LOGGER, ErrorContext, Logger, SILENT_LOGGER, set_logger};
pub(crate) use rustcodegraph::sync::watcher::{
    __emit_watch_event_for_tests, __set_fs_watch_for_tests,
    __set_supports_recursive_watch_for_tests, FileWatcher, LockUnavailableError, SyncRunResult,
    WatchErrorHandler, WatchHandle, WatchOptions as FileWatchOptions, WatchStartError,
    WatchSyncError,
};
pub(crate) use rustcodegraph::{CodeGraph, IndexOptions, WatchOptions as CodeGraphWatchOptions};

static TEST_MUTEX: OnceLock<Mutex<()>> = OnceLock::new();

pub(crate) struct TestGuard {
    _lock: MutexGuard<'static, ()>,
    old_node_env: Option<OsString>,
}

impl TestGuard {
    pub(crate) fn new() -> Self {
        let lock = TEST_MUTEX
            .get_or_init(|| Mutex::new(()))
            .lock()
            .expect("watcher test mutex should not be poisoned");
        let old_node_env = env::var_os("NODE_ENV");
        unsafe {
            env::set_var("NODE_ENV", "test");
        }
        __set_fs_watch_for_tests(None);
        __set_supports_recursive_watch_for_tests(None);
        set_logger(SILENT_LOGGER);
        Self {
            _lock: lock,
            old_node_env,
        }
    }
}

impl Drop for TestGuard {
    fn drop(&mut self) {
        __set_fs_watch_for_tests(None);
        __set_supports_recursive_watch_for_tests(None);
        set_logger(DEFAULT_LOGGER);
        match &self.old_node_env {
            Some(value) => unsafe {
                env::set_var("NODE_ENV", value);
            },
            None => unsafe {
                env::remove_var("NODE_ENV");
            },
        }
    }
}

pub(crate) struct TempProject {
    root: PathBuf,
}

impl TempProject {
    pub(crate) fn new(prefix: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after Unix epoch")
            .as_nanos();
        let root = env::temp_dir().join(format!("{prefix}-{}-{nanos}", std::process::id()));
        fs::create_dir_all(root.join("src")).expect("fixture src directory should be created");
        fs::write(root.join("src").join("index.ts"), "export const x = 1;")
            .expect("fixture source file should be written");
        Self { root }
    }

    pub(crate) fn empty(prefix: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after Unix epoch")
            .as_nanos();
        let root = env::temp_dir().join(format!("{prefix}-{}-{nanos}", std::process::id()));
        fs::create_dir_all(&root).expect("fixture directory should be created");
        Self { root }
    }

    pub(crate) fn path(&self) -> &Path {
        &self.root
    }
}

impl Drop for TempProject {
    fn drop(&mut self) {
        if self.root.exists() {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}

#[derive(Clone)]
struct CaptureLogger {
    warnings: Arc<Mutex<Vec<String>>>,
}

impl Logger for CaptureLogger {
    fn debug(&self, _message: &str, _context: Option<&ErrorContext>) {}

    fn warn(&self, message: &str, _context: Option<&ErrorContext>) {
        self.warnings
            .lock()
            .expect("warnings lock should not be poisoned")
            .push(message.to_owned());
    }

    fn error(&self, _message: &str, _context: Option<&ErrorContext>) {}
}

pub(crate) fn capture_warnings() -> Arc<Mutex<Vec<String>>> {
    let warnings = Arc::new(Mutex::new(Vec::new()));
    set_logger(CaptureLogger {
        warnings: Arc::clone(&warnings),
    });
    warnings
}

pub(crate) fn warning_count(warnings: &Arc<Mutex<Vec<String>>>, needle: &str) -> usize {
    warnings
        .lock()
        .expect("warnings lock should not be poisoned")
        .iter()
        .filter(|message| message.contains(needle))
        .count()
}

pub(crate) fn warning_with(warnings: &Arc<Mutex<Vec<String>>>, needle: &str) -> Option<String> {
    warnings
        .lock()
        .expect("warnings lock should not be poisoned")
        .iter()
        .find(|message| message.contains(needle))
        .cloned()
}

pub(crate) fn ok(files_changed: usize, duration_ms: u64) -> SyncRunResult {
    SyncRunResult {
        files_changed,
        duration_ms,
    }
}

pub(crate) fn sync_mock(
    outcomes: Vec<Result<SyncRunResult, WatchSyncError>>,
    default: Result<SyncRunResult, WatchSyncError>,
) -> (
    Arc<AtomicUsize>,
    impl FnMut() -> Result<SyncRunResult, WatchSyncError> + Send + 'static,
) {
    let calls = Arc::new(AtomicUsize::new(0));
    let seen_calls = Arc::clone(&calls);
    let mut outcomes = VecDeque::from(outcomes);
    (calls, move || {
        seen_calls.fetch_add(1, Ordering::SeqCst);
        outcomes.pop_front().unwrap_or_else(|| default.clone())
    })
}

pub(crate) fn inert_options(debounce_ms: u64) -> FileWatchOptions {
    FileWatchOptions {
        debounce_ms: Some(debounce_ms),
        inert_for_tests: true,
        ..FileWatchOptions::default()
    }
}

pub(crate) fn wait_for<F>(mut condition: F, timeout_ms: u64, interval_ms: u64)
where
    F: FnMut() -> bool,
{
    let start = Instant::now();
    let timeout = Duration::from_millis(timeout_ms);
    loop {
        if condition() {
            return;
        }
        assert!(start.elapsed() <= timeout, "waitFor timed out");
        thread::sleep(Duration::from_millis(interval_ms));
    }
}

pub(crate) struct CountingWatchHandle {
    pub(crate) closed: Arc<AtomicUsize>,
    pub(crate) error_handler: Arc<Mutex<Option<WatchErrorHandler>>>,
}

impl WatchHandle for CountingWatchHandle {
    fn close(&mut self) {
        self.closed.fetch_add(1, Ordering::SeqCst);
    }

    fn set_error_handler(&mut self, handler: WatchErrorHandler) {
        *self
            .error_handler
            .lock()
            .expect("error handler lock should not be poisoned") = Some(handler);
    }
}
