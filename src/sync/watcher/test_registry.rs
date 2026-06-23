//! watcher 测试事件 registry。
//!
//! 真实 watcher 的 OS 事件不可控，测试通过这个 registry 把合成事件注入到
//! 当前 live watcher；没有 live watcher 时退回 facade registry。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

use super::facade::ingest_facade_event_for_tests;
use super::state::FileWatcher;
use super::util::watch_registry_key;

static LIVE_WATCHERS_FOR_TESTS: LazyLock<Mutex<HashMap<PathBuf, usize>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

impl FileWatcher {
    pub(super) fn register_live_watcher_for_tests(&mut self) {
        // 只在测试运行时登记裸指针；start/stop/drop 保证登记窗口跟 watcher 生命周期一致。
        if let Ok(mut watchers) = LIVE_WATCHERS_FOR_TESTS.lock() {
            watchers.insert(
                watch_registry_key(&self.project_root),
                self as *mut Self as usize,
            );
        }
    }

    pub(super) fn unregister_live_watcher_for_tests(&mut self) {
        if let Ok(mut watchers) = LIVE_WATCHERS_FOR_TESTS.lock() {
            watchers.remove(&watch_registry_key(&self.project_root));
        }
    }
}

/// 测试专用：为 live watcher 合成一次源文件变更。
pub fn __emit_watch_event_for_tests(
    project_root: impl AsRef<Path>,
    rel_path: impl AsRef<str>,
) -> bool {
    let key = project_root.as_ref().to_path_buf();
    let key = watch_registry_key(&key);
    let ptr = LIVE_WATCHERS_FOR_TESTS
        .lock()
        .ok()
        .and_then(|watchers| watchers.get(&key).copied());
    let Some(ptr) = ptr else {
        return ingest_facade_event_for_tests(&key, rel_path.as_ref());
    };

    // 与 TypeScript 测试 registry 一致：start() 插入，stop()/drop() 移除，
    // 且只在测试运行时存在，所以这里可以把指针还原成 watcher。
    let watcher = unsafe { (ptr as *mut FileWatcher).as_mut() };
    if let Some(watcher) = watcher {
        watcher.ingest_event_for_tests(rel_path.as_ref());
        true
    } else {
        ingest_facade_event_for_tests(&key, rel_path.as_ref())
    }
}
