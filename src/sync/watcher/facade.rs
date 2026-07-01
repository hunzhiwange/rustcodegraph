//! watcher 状态的进程内 facade registry。
//!
//! `CodeGraph` public API 和 MCP status 查询可能重新打开一个 facade 实例，
//! 但真实 watcher 只由原实例持有。这个 registry 把 active/degraded/pending
//! 状态镜像出来，让同进程的查询看到一致视图。

use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::{LazyLock, Mutex};

use crate::directory::is_code_graph_data_dir;
use crate::extraction::grammars::is_source_file;
use crate::utils::normalize_path;

use super::backend::{WatchStartError, create_watch_handle, supports_recursive_watch};
use super::types::{EXHAUSTION_REASON, PendingFile, PendingInfo};
use super::util::{now_ms, watch_registry_key};

static FACADE_WATCHERS: LazyLock<Mutex<HashMap<std::path::PathBuf, FacadeWatchState>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub fn register_facade_watcher(project_root: impl AsRef<Path>, debounce_ms: u64) -> bool {
    let root = watch_registry_key(project_root.as_ref());
    let mut state = FacadeWatchState {
        active: true,
        debounce_ms,
        pending_files: BTreeMap::new(),
        degraded_reason: None,
        auto_clear_after_debounce: true,
    };

    match create_watch_handle(&root, supports_recursive_watch()) {
        // 这个路径只做“能否安装 watcher”的轻量探测；真正事件由 runtime watcher 处理。
        Ok(mut handle) => handle.close(),
        Err(WatchStartError::ResourceExhaustion(_)) => {
            state.active = false;
            state.degraded_reason = Some(EXHAUSTION_REASON.to_string());
        }
        Err(WatchStartError::InotifyExhaustion(message) | WatchStartError::Other(message)) => {
            state.active = false;
            state.degraded_reason = Some(message);
        }
    }

    let active = state.active;
    if let Ok(mut watchers) = FACADE_WATCHERS.lock() {
        watchers.insert(root, state);
    }
    active
}

pub fn register_facade_runtime_watcher(project_root: impl AsRef<Path>, debounce_ms: u64) {
    // runtime watcher 会主动 update；因此 pending 不按 debounce 自动清空。
    let root = watch_registry_key(project_root.as_ref());
    let state = FacadeWatchState {
        active: true,
        debounce_ms,
        pending_files: BTreeMap::new(),
        degraded_reason: None,
        auto_clear_after_debounce: false,
    };
    if let Ok(mut watchers) = FACADE_WATCHERS.lock() {
        watchers.insert(root, state);
    }
}

pub fn update_facade_runtime_watcher(
    project_root: impl AsRef<Path>,
    active: bool,
    degraded_reason: Option<String>,
    pending_files: Vec<PendingFile>,
) {
    // 运行线程周期性把真实 watcher 的快照写回 registry；读取端不需要持有
    // RuntimeFileWatcher 的锁，也能给 MCP 工具追加 stale/degraded 提示。
    let key = watch_registry_key(project_root.as_ref());
    let Ok(mut watchers) = FACADE_WATCHERS.lock() else {
        return;
    };
    let Some(state) = watchers.get_mut(&key) else {
        return;
    };
    state.active = active;
    state.degraded_reason = degraded_reason;
    state.pending_files = pending_files
        .into_iter()
        .map(|pending| {
            (
                pending.path,
                PendingInfo {
                    first_seen_ms: pending.first_seen_ms,
                    last_seen_ms: pending.last_seen_ms,
                    indexing: pending.indexing,
                },
            )
        })
        .collect();
}

pub fn unregister_facade_watcher(project_root: impl AsRef<Path>) {
    if let Ok(mut watchers) = FACADE_WATCHERS.lock() {
        watchers.remove(&watch_registry_key(project_root.as_ref()));
    }
}

pub fn facade_pending_files(project_root: impl AsRef<Path>) -> Vec<PendingFile> {
    // 非 runtime 的 facade 探测没有真实同步线程，所以在 debounce 窗口后本地清空，
    // 模拟“事件已被同步消费”的旧 API 行为。
    let Ok(mut watchers) = FACADE_WATCHERS.lock() else {
        return Vec::new();
    };
    let key = watch_registry_key(project_root.as_ref());
    let Some(state) = watchers.get_mut(&key) else {
        return Vec::new();
    };
    flush_facade_due(state);
    state
        .pending_files
        .iter()
        .map(|(path, info)| PendingFile {
            path: path.clone(),
            first_seen_ms: info.first_seen_ms,
            last_seen_ms: info.last_seen_ms,
            indexing: info.indexing,
        })
        .collect()
}

pub fn facade_degraded_reason(project_root: impl AsRef<Path>) -> Option<String> {
    let key = watch_registry_key(project_root.as_ref());
    FACADE_WATCHERS.lock().ok().and_then(|watchers| {
        watchers
            .get(&key)
            .and_then(|state| state.degraded_reason.clone())
    })
}

pub(super) fn ingest_facade_event_for_tests(project_root: &Path, rel_path: &str) -> bool {
    // 当测试没有 live watcher 指针时，事件落到 facade registry，覆盖重新打开
    // CodeGraph 后仍能看到 pending 文件的场景。
    let Ok(mut watchers) = FACADE_WATCHERS.lock() else {
        return false;
    };
    let Some(state) = watchers.get_mut(project_root) else {
        return false;
    };
    if !state.active || state.degraded_reason.is_some() {
        return false;
    }
    let rel = normalize_path(rel_path);
    if rel.is_empty()
        || rel == "."
        || rel.starts_with("..")
        || rel.split('/').next().is_some_and(is_code_graph_data_dir)
        || !is_source_file(&rel)
    {
        return false;
    }
    let now = now_ms();
    let first_seen_ms = state
        .pending_files
        .get(&rel)
        .map(|info| info.first_seen_ms)
        .unwrap_or(now);
    state.pending_files.insert(
        rel,
        PendingInfo {
            first_seen_ms,
            last_seen_ms: now,
            indexing: false,
        },
    );
    true
}

fn flush_facade_due(state: &mut FacadeWatchState) {
    if !state.auto_clear_after_debounce
        || state.pending_files.is_empty()
        || state.degraded_reason.is_some()
    {
        return;
    }
    // 以最新事件为基准，避免事件风暴中较早文件先被清掉。
    let newest = state
        .pending_files
        .values()
        .map(|info| info.last_seen_ms)
        .max()
        .unwrap_or(0);
    if now_ms().saturating_sub(newest) >= state.debounce_ms as i64 {
        state.pending_files.clear();
    }
}

#[derive(Debug, Default)]
struct FacadeWatchState {
    /// 是否认为该 project root 仍有 watcher 覆盖。
    active: bool,
    /// facade 自清理 pending 使用的 debounce 窗口。
    debounce_ms: u64,
    /// project-relative POSIX path 到事件时间戳。
    pending_files: BTreeMap<String, PendingInfo>,
    /// watcher 永久降级原因；非空时索引可能整体落后。
    degraded_reason: Option<String>,
    /// runtime watcher 自己负责清理 pending，probe facade 才需要自动清空。
    auto_clear_after_debounce: bool,
}
