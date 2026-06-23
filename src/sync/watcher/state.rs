//! `FileWatcher` 的共享状态定义。
//!
//! 方法按职责拆在 lifecycle/events/sync 等模块里，这里只集中声明字段，方便
//! 维护者看到状态机完整形状。

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::time::Instant;

use crate::extraction::index::ScopeIgnore;

use super::backend::WatchHandle;
use super::types::{
    DegradedCallback, PendingInfo, SyncCompleteCallback, SyncErrorCallback, SyncFn,
};

/// 监听项目目录变化，并触发 debounce 后的增量同步。
pub struct FileWatcher {
    // OS watch 句柄：递归模式只有一个，Linux 逐目录模式按目录持有多个。
    pub(super) recursive_watcher: Option<Box<dyn WatchHandle>>,
    pub(super) dir_watchers: HashMap<PathBuf, Box<dyn WatchHandle>>,
    // 资源告警/降级状态。degraded_reason 一旦写入，live watching 视为永久关闭。
    pub(super) dir_cap_warned: bool,
    pub(super) inotify_limit_warned: bool,
    pub(super) degraded_reason: Option<String>,
    // 同步调度状态：pending_files 记录事件，scheduled_* 记录 debounce/backoff。
    pub(super) lock_retry_count: usize,
    pub(super) inert: bool,
    pub(super) scheduled_at: Option<Instant>,
    pub(super) scheduled_delay_ms: Option<u64>,
    pub(super) pending_files: BTreeMap<String, PendingInfo>,
    // 正在同步时用于区分“本轮已覆盖”和“同步开始后又发生”的文件事件。
    pub(super) sync_started_ms: i64,
    pub(super) syncing: bool,
    // 生命周期与过滤配置。
    pub(super) stopped: bool,
    pub(super) ready: bool,
    pub(super) ignore_matcher: Option<ScopeIgnore>,

    // 固定配置和回调。
    pub(super) project_root: PathBuf,
    pub(super) debounce_ms: u64,
    pub(super) sync_fn: SyncFn,
    pub(super) on_sync_complete: Option<SyncCompleteCallback>,
    pub(super) on_sync_error: Option<SyncErrorCallback>,
    pub(super) on_degraded: Option<DegradedCallback>,
    pub(super) inert_for_tests: bool,
}
