//! watcher 对外类型和状态机常量。
//!
//! 这些类型被 `CodeGraph` facade 重新导出，字段语义需要保持稳定，MCP stale
//! 提示和 SDK 调用方都会依赖它们。

use std::error::Error;
use std::fmt;

// 写锁竞争允许短暂存在；超过预算说明自动同步长期无法推进。
pub(super) const MAX_LOCK_RETRIES: usize = 5;
pub(super) const MAX_LOCK_RETRY_DELAY_MS: u64 = 30_000;
// Linux 逐目录监听的进程内保护上限，可由环境变量覆盖。
pub(super) const DEFAULT_MAX_DIR_WATCHES: usize = 50_000;

// 这些原因字符串会直接展示给用户，因此保持可操作而不是内部错误码。
pub(super) const EXHAUSTION_REASON: &str = "OS watch/file limit exhausted; auto-sync disabled. Run \
`rustcodegraph sync` (or install git sync hooks) to refresh the graph after changes.";

pub(super) const INOTIFY_LIMIT_REASON: &str = "Linux inotify watch limit reached \
(fs.inotify.max_user_watches); live watching now covers only part of the project, so edits \
in unwatched directories will not auto-sync. Raise the limit (e.g. `sudo sysctl \
fs.inotify.max_user_watches=1048576`, persisted in /etc/sysctl.d) and restart, or run \
`rustcodegraph sync` (or install git sync hooks) to refresh.";

pub(super) type SyncFn = Box<dyn FnMut() -> Result<SyncRunResult, WatchSyncError> + Send>;
pub type SyncCompleteCallback = Box<dyn Fn(&SyncRunResult) + Send + Sync>;
pub type SyncErrorCallback = Box<dyn Fn(&WatchSyncError) + Send + Sync>;
pub type DegradedCallback = Box<dyn Fn(&str) + Send + Sync>;

/// watcher 触发同步后的回调结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyncRunResult {
    pub files_changed: usize,
    pub duration_ms: u64,
}

/// 文件 watcher 配置。
#[derive(Default)]
pub struct WatchOptions {
    /// debounce 延迟，单位毫秒；默认 2000ms。
    pub debounce_ms: Option<u64>,
    /// 同步成功完成时回调。
    pub on_sync_complete: Option<SyncCompleteCallback>,
    /// 同步返回普通错误时回调。
    pub on_sync_error: Option<SyncErrorCallback>,
    /// live watching 永久降级时只触发一次。
    pub on_degraded: Option<DegradedCallback>,
    /// 测试专用 inert 模式：启动状态机但不安装 OS watcher。
    pub inert_for_tests: bool,
}

/// sync 函数在跨进程写锁不可用时返回的错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockUnavailableError {
    message: String,
}

impl LockUnavailableError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl Default for LockUnavailableError {
    fn default() -> Self {
        Self::new("RustCodeGraph file lock unavailable; another process is writing")
    }
}

impl fmt::Display for LockUnavailableError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl Error for LockUnavailableError {}

/// watcher 触发同步时可能返回的错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WatchSyncError {
    /// 另一个进程正在写索引；watcher 会按退避策略重试。
    LockUnavailable(LockUnavailableError),
    /// 非锁竞争错误；保留 pending，交给下一轮同步继续尝试。
    Other(String),
}

impl WatchSyncError {
    pub(super) fn message(&self) -> &str {
        match self {
            Self::LockUnavailable(err) => err.message(),
            Self::Other(message) => message,
        }
    }
}

impl fmt::Display for WatchSyncError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.message())
    }
}

impl Error for WatchSyncError {}

impl From<LockUnavailableError> for WatchSyncError {
    fn from(value: LockUnavailableError) -> Self {
        Self::LockUnavailable(value)
    }
}

/// 单个 pending 文件条目。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingFile {
    /// project-relative POSIX 路径。
    pub path: String,
    /// 上次成功同步后第一次看到事件的 wall-clock 毫秒。
    pub first_seen_ms: i64,
    /// 最近一次事件的 wall-clock 毫秒。
    pub last_seen_ms: i64,
    /// 当前进行中的同步是否已经覆盖该文件最近一次事件。
    pub indexing: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum WatchMode {
    /// 平台支持的递归监听。
    Recursive,
    /// Linux 等平台逐目录安装 watch。
    PerDirectory,
    /// 测试模式，不触碰真实 OS watcher。
    Inert,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct PendingInfo {
    /// pending 队列里的首次事件时间，用于 stale 提示年龄。
    pub(super) first_seen_ms: i64,
    /// pending 清理以最后事件时间为准，避免同步期间的新事件被误删。
    pub(super) last_seen_ms: i64,
}
