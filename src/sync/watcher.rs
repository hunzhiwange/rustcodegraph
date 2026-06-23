//! File watcher state machine translated from `watcher.ts`.
//!
//! Runtime wiring uses native filesystem notifications through `notify`, while
//! keeping the public API, pending-file queue, degradation, lock retry/backoff
//! behavior, and the test-only seams from the TypeScript watcher.
//!
//! 中文维护提示：这个文件只是 facade；真正的状态机被拆到子模块，re-export 保持
//! 旧路径兼容，避免调用方感知 watcher 的内部拆分。

mod backend;
mod events;
mod facade;
mod lifecycle;
mod state;
mod sync;
mod test_registry;
mod types;
mod util;

pub use backend::{
    __set_fs_watch_for_tests, __set_supports_recursive_watch_for_tests, WatchErrorHandler,
    WatchFactory, WatchHandle, WatchStartError,
};
pub use facade::{
    facade_degraded_reason, facade_pending_files, register_facade_runtime_watcher,
    register_facade_watcher, unregister_facade_watcher, update_facade_runtime_watcher,
};
pub use state::FileWatcher;
pub use test_registry::__emit_watch_event_for_tests;
pub use types::{
    DegradedCallback, LockUnavailableError, PendingFile, SyncCompleteCallback, SyncErrorCallback,
    SyncRunResult, WatchOptions, WatchSyncError,
};
