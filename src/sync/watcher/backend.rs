//! 文件监听底层适配层。
//!
//! 上层 watcher 状态机只认识 `WatchHandle`，这里负责把 `notify` 的平台差异、
//! 运行时错误和测试注入点收束成稳定的小接口。

use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};

use notify::{Config as NotifyConfig, RecommendedWatcher, RecursiveMode, Watcher};

use super::types::DEFAULT_MAX_DIR_WATCHES;

/// OS watcher 的最小抽象：关闭、运行时错误回调，以及把底层事件批量取出。
///
/// 事件先进入队列，再由 `FileWatcher::tick` 消费，避免 notify 回调线程直接
/// 驱动索引同步或修改大块 watcher 状态。
pub trait WatchHandle: Send {
    fn close(&mut self);

    fn set_error_handler(&mut self, _handler: WatchErrorHandler) {}

    fn take_events(&mut self) -> Vec<PathBuf> {
        Vec::new()
    }
}

pub type WatchErrorHandler = Box<dyn Fn(WatchStartError) + Send + Sync>;

struct NativeWatchHandle {
    /// 持有 `RecommendedWatcher` 本身；`take()` 即可触发底层资源释放。
    watcher: Option<RecommendedWatcher>,
    /// notify 回调可能来自后台线程，因此只把 path 追加到队列。
    events: Arc<Mutex<Vec<PathBuf>>>,
    /// 错误回调要晚于 handle 创建后安装，所以用可替换 slot。
    error_handler: Arc<Mutex<Option<WatchErrorHandler>>>,
}

impl NativeWatchHandle {
    fn new(dir: &Path, recursive: bool) -> Result<Self, WatchStartError> {
        let events = Arc::new(Mutex::new(Vec::new()));
        let error_handler: Arc<Mutex<Option<WatchErrorHandler>>> = Arc::new(Mutex::new(None));
        let event_queue = Arc::clone(&events);
        let error_sink = Arc::clone(&error_handler);

        let mut watcher = RecommendedWatcher::new(
            move |result: notify::Result<notify::Event>| match result {
                Ok(event) => {
                    // notify 一次事件可能携带多个路径；保留原始路径，归一化交给上层。
                    if let Ok(mut queue) = event_queue.lock() {
                        queue.extend(event.paths);
                    }
                }
                Err(err) => {
                    if let Ok(handler) = error_sink.lock()
                        && let Some(handler) = handler.as_ref()
                    {
                        handler(watch_error_from_message(err.to_string()));
                    }
                }
            },
            NotifyConfig::default(),
        )
        .map_err(|err| watch_error_from_message(err.to_string()))?;

        watcher
            .watch(
                dir,
                if recursive {
                    RecursiveMode::Recursive
                } else {
                    RecursiveMode::NonRecursive
                },
            )
            .map_err(|err| watch_error_from_message(err.to_string()))?;

        Ok(Self {
            watcher: Some(watcher),
            events,
            error_handler,
        })
    }
}

impl WatchHandle for NativeWatchHandle {
    fn close(&mut self) {
        self.watcher.take();
    }

    fn set_error_handler(&mut self, handler: WatchErrorHandler) {
        if let Ok(mut slot) = self.error_handler.lock() {
            *slot = Some(handler);
        }
    }

    fn take_events(&mut self) -> Vec<PathBuf> {
        let Ok(mut events) = self.events.lock() else {
            return Vec::new();
        };
        std::mem::take(&mut *events)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WatchStartError {
    /// 通用文件描述符/句柄耗尽，继续自动监听通常只会反复失败。
    ResourceExhaustion(String),
    /// Linux inotify watch 数耗尽；可通过调高内核限额恢复。
    InotifyExhaustion(String),
    /// 其它平台或 notify 错误，通常只影响当前 watch 入口。
    Other(String),
}

impl WatchStartError {
    pub fn resource_exhaustion(message: impl Into<String>) -> Self {
        Self::ResourceExhaustion(message.into())
    }

    pub fn inotify_exhaustion(message: impl Into<String>) -> Self {
        Self::InotifyExhaustion(message.into())
    }

    pub fn other(message: impl Into<String>) -> Self {
        Self::Other(message.into())
    }

    pub(super) fn message(&self) -> &str {
        match self {
            Self::ResourceExhaustion(message)
            | Self::InotifyExhaustion(message)
            | Self::Other(message) => message,
        }
    }
}

fn watch_error_from_message(message: String) -> WatchStartError {
    // notify 把很多平台错误折叠成字符串；这里用保守分类决定上层是永久降级、
    // Linux 局部告警，还是只移除单个失败 watch。
    if message.contains("ENOSPC") || message.contains("inotify") {
        WatchStartError::inotify_exhaustion(message)
    } else if message.contains("EMFILE")
        || message.contains("ENFILE")
        || message.to_ascii_lowercase().contains("too many open files")
    {
        WatchStartError::resource_exhaustion(message)
    } else {
        WatchStartError::other(message)
    }
}

pub type WatchFactory =
    Arc<dyn Fn(&Path) -> Result<Box<dyn WatchHandle>, WatchStartError> + Send + Sync>;

static WATCH_FACTORY: LazyLock<Mutex<Option<WatchFactory>>> = LazyLock::new(|| Mutex::new(None));
static RECURSIVE_WATCH_OVERRIDE: LazyLock<Mutex<Option<bool>>> = LazyLock::new(|| Mutex::new(None));

/// 测试注入假的 watch 实现，覆盖资源耗尽、事件队列等不稳定的 OS 行为。
pub fn __set_fs_watch_for_tests(factory: Option<WatchFactory>) {
    if let Ok(mut slot) = WATCH_FACTORY.lock() {
        *slot = factory;
    }
}

/// 测试强制递归或逐目录策略，避免 CI 平台差异遮住状态机分支。
pub fn __set_supports_recursive_watch_for_tests(value: Option<bool>) {
    if let Ok(mut slot) = RECURSIVE_WATCH_OVERRIDE.lock() {
        *slot = value;
    }
}

pub(super) fn create_watch_handle(
    dir: &Path,
    recursive: bool,
) -> Result<Box<dyn WatchHandle>, WatchStartError> {
    let factory = WATCH_FACTORY
        .lock()
        .ok()
        .and_then(|slot| slot.as_ref().map(Arc::clone));
    if let Some(factory) = factory {
        // fake factory 不接收 recursive 参数，测试只关心状态机如何响应结果。
        factory(dir)
    } else {
        NativeWatchHandle::new(dir, recursive)
            .map(|handle| Box::new(handle) as Box<dyn WatchHandle>)
    }
}

pub(super) fn supports_recursive_watch() -> bool {
    if let Ok(slot) = RECURSIVE_WATCH_OVERRIDE.lock()
        && let Some(value) = *slot
    {
        return value;
    }
    // macOS FSEvents 和 Windows RDCW 能稳定递归监听；Linux 走逐目录，
    // 这样才能显式处理 inotify watch 配额和新建子目录。
    cfg!(target_os = "macos") || cfg!(target_os = "windows")
}

pub(super) fn max_dir_watches() -> usize {
    // 逐目录策略需要软上限，避免极大仓库把进程或系统 watch 资源打满。
    std::env::var("RUSTCODEGRAPH_MAX_DIR_WATCHES")
        .ok()
        .and_then(|raw| raw.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_MAX_DIR_WATCHES)
}
