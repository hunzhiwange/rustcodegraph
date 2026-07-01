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

// 两次重型同步之间的最小间隔上限：支持一分钟级批量同步，同时即便配置/默认值更大，
// 也保证 pending 不会被节流永久饿死——到这个上限就必须放行。
pub(super) const MAX_MIN_SYNC_INTERVAL_MS: u64 = 60_000;

// max-wait 默认取 debounce 的这个倍数：在合并抖动（debounce 正常工作）和持续事件流
// 下不至于饿死之间折中。默认 debounce 2000ms → max-wait 10s。
const DEFAULT_MAX_DEBOUNCE_MULTIPLIER: u64 = 5;

// max-wait 的硬上限：支持一分钟级批量同步，同时即便配置/默认值更大，持续事件流下也
// 保证最迟到此值放行一次。
pub(super) const MAX_MAX_DEBOUNCE_MS: u64 = 60_000;

/// 解析 debounce 的最大等待上限（毫秒）。环境变量由 CLI/MCP facade 统一解析后填入
/// 显式选项；runtime watcher 只消费 option 和默认值，避免同一个配置入口被二次解析。
/// 结果至少为 `debounce_ms`（max-wait 比单次 debounce 还短没有意义），并按
/// `MAX_MAX_DEBOUNCE_MS` 截顶，保证持续事件流下 pending 不会被无限重置饿死。
pub(super) fn resolve_max_debounce_ms(option: Option<u64>, debounce_ms: u64) -> u64 {
    let resolved =
        option.unwrap_or_else(|| debounce_ms.saturating_mul(DEFAULT_MAX_DEBOUNCE_MULTIPLIER));
    resolved.max(debounce_ms).min(MAX_MAX_DEBOUNCE_MS)
}

/// 解析最小同步间隔（毫秒）。环境变量由 CLI/MCP facade 统一解析后填入显式选项；
/// runtime watcher 只消费 option 和默认值。默认取 `debounce_ms`，保守地让节流至少和
/// 一次 debounce 窗口对齐。结果按 `MAX_MIN_SYNC_INTERVAL_MS` 截顶，保证 pending 不会被
/// 节流永久饿死。
pub(super) fn resolve_min_sync_interval_ms(option: Option<u64>, debounce_ms: u64) -> u64 {
    let resolved = option.unwrap_or(debounce_ms);
    resolved.min(MAX_MIN_SYNC_INTERVAL_MS)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ScheduledSyncKind {
    Debounce,
    Retry,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct FileSnapshot {
    pub modified_ns: u128,
    pub len: u64,
}

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
    /// 本轮同步因内存保护跳过了重型解析（OOM 保护）。watcher 据此**不**清理
    /// pending、保持 active、并安排稍后重试，而不是把这次跳过当作正常完成或错误。
    pub skipped: bool,
}

/// 文件 watcher 配置。
#[derive(Default)]
pub struct WatchOptions {
    /// debounce 延迟，单位毫秒；默认 2000ms。
    pub debounce_ms: Option<u64>,
    /// 两次重型同步之间的最小间隔（毫秒）。debounce 合并抖动，这里再防止一轮同步
    /// 刚结束、内存还没回落就被下一波 pending 背靠背触发。默认取 `debounce_ms`，
    /// public facade 可通过 `RUSTCODEGRAPH_WATCH_MIN_SYNC_INTERVAL_MS` 填入该字段；有上限，
    /// 不会饿死 pending。内存保护跳过后的慢重试也会至少等待这个窗口。
    pub min_sync_interval_ms: Option<u64>,
    /// debounce 的最大等待上限（毫秒）。纯 debounce 在持续事件流（如复制一个大文件夹）
    /// 下会被无限重置导致 sync 永不触发；这个上限保证从本轮第一个事件起，最迟到此值
    /// 就放行一次 flush。默认取 `debounce_ms` 的若干倍，可由
    /// public facade 通过 `RUSTCODEGRAPH_WATCH_MAX_DEBOUNCE_MS` 填入。内存保护跳过后的
    /// 慢重试会复用该批量窗口，避免高内存状态下按短 debounce 空转。
    pub max_debounce_ms: Option<u64>,
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
    /// facade registry 会镜像 runtime watcher 的 indexing 状态，用于用户提示。
    pub(super) indexing: bool,
}
