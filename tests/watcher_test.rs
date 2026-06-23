//! FileWatcher tests.
//!
//! This is the Rust port of `__tests__/watcher.test.ts`. The Rust watcher
//! translation exposes explicit test seams instead of async timers, so the
//! tests drive synthetic events and flush the recorded debounce/backoff work
//! deterministically.

#[path = "watcher_test/common.rs"]
mod common;
#[path = "watcher_test/file_watcher.rs"]
mod file_watcher;
