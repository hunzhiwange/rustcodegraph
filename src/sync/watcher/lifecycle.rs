//! `FileWatcher` 生命周期入口。
//!
//! 这个模块负责构造、启动/停止、测试注入、ready 等待和 tick 驱动；事件过滤
//! 与同步重试分别在 `events.rs` 和 `sync.rs`。

use std::collections::{BTreeMap, HashMap};
use std::time::{Duration, Instant};

use serde_json::json;

use crate::errors::log_debug;
use crate::extraction::index::build_scope_ignore;
use crate::utils::normalize_path;

use super::backend::supports_recursive_watch;
use super::state::FileWatcher;
use super::types::{
    PendingFile, SyncRunResult, WatchMode, WatchOptions, WatchSyncError, resolve_max_debounce_ms,
    resolve_min_sync_interval_ms,
};
use super::util::{context, is_test_runtime, path_string};

impl FileWatcher {
    /// 构造 watcher 状态机；此时不安装任何 OS watch。
    pub fn new<F>(
        project_root: impl Into<std::path::PathBuf>,
        sync_fn: F,
        options: WatchOptions,
    ) -> Self
    where
        F: FnMut() -> Result<SyncRunResult, WatchSyncError> + Send + 'static,
    {
        let debounce_ms = options.debounce_ms.unwrap_or(2000);
        let min_sync_interval_ms =
            resolve_min_sync_interval_ms(options.min_sync_interval_ms, debounce_ms);
        let max_debounce_ms = resolve_max_debounce_ms(options.max_debounce_ms, debounce_ms);
        Self {
            recursive_watcher: None,
            dir_watchers: HashMap::new(),
            dir_cap_warned: false,
            inotify_limit_warned: false,
            degraded_reason: None,
            lock_retry_count: 0,
            inert: false,
            scheduled_at: None,
            scheduled_delay_ms: None,
            scheduled_kind: None,
            debounce_started_at: None,
            pending_files: BTreeMap::new(),
            poll_snapshot: BTreeMap::new(),
            last_poll_at: None,
            poll_interval_ms: debounce_ms.clamp(250, 2000),
            sync_started_ms: 0,
            syncing: false,
            last_sync_finished_at: None,
            min_sync_interval_ms,
            max_debounce_ms,
            stopped: false,
            ready: false,
            ignore_matcher: None,
            project_root: project_root.into(),
            debounce_ms,
            sync_fn: Box::new(sync_fn),
            on_sync_complete: options.on_sync_complete,
            on_sync_error: options.on_sync_error,
            on_degraded: options.on_degraded,
            inert_for_tests: options.inert_for_tests,
        }
    }

    /// 启动文件监听，并根据平台选择递归或逐目录策略。
    pub fn start(&mut self) -> bool {
        if self.recursive_watcher.is_some() || !self.dir_watchers.is_empty() || self.inert {
            return true;
        }

        self.stopped = false;
        self.degraded_reason = None;
        self.lock_retry_count = 0;
        self.scheduled_at = None;
        self.scheduled_delay_ms = None;
        self.scheduled_kind = None;
        self.debounce_started_at = None;
        self.last_sync_finished_at = None;
        self.last_poll_at = None;

        if let Some(disabled_reason) =
            super::super::watch_policy::watch_disabled_reason(&self.project_root, None)
        {
            // watch policy 负责资源/环境禁用判断；这里保持“未启动但非错误”的语义。
            let ctx = context([
                ("reason", json!(disabled_reason)),
                ("projectRoot", json!(path_string(&self.project_root))),
            ]);
            log_debug("File watcher disabled", Some(&ctx));
            return false;
        }

        self.ignore_matcher = Some(build_scope_ignore(&self.project_root, None::<Vec<String>>));
        self.poll_snapshot = self.collect_source_snapshot();
        self.last_poll_at = Some(Instant::now());

        let mode = if self.inert_for_tests {
            // inert 模式只跑状态机，不触碰真实文件系统 watcher，便于稳定测试 debounce。
            self.inert = true;
            WatchMode::Inert
        } else if supports_recursive_watch() {
            if let Err(err) = self.start_recursive() {
                self.handle_start_error(err);
                return false;
            }
            WatchMode::Recursive
        } else {
            self.start_per_directory();
            WatchMode::PerDirectory
        };
        if self.degraded_reason.is_some() {
            return false;
        }

        self.pending_files.clear();
        // ready 放在 watch 安装完成之后，避免测试或 MCP catch-up gate 抢在事件过滤
        // 所需状态初始化前继续执行。
        self.ready = true;
        if is_test_runtime() {
            self.register_live_watcher_for_tests();
        }

        let mode = match mode {
            WatchMode::Inert => "inert",
            WatchMode::Recursive => "recursive",
            WatchMode::PerDirectory => "per-directory",
        };
        let mut ctx = context([
            ("projectRoot", json!(path_string(&self.project_root))),
            ("debounceMs", json!(self.debounce_ms)),
            ("mode", json!(mode)),
        ]);
        if !self.dir_watchers.is_empty() {
            ctx.insert("watchedDirs".to_owned(), json!(self.dir_watchers.len()));
        }
        log_debug("File watcher started", Some(&ctx));
        true
    }

    /// 停止文件监听并清理所有运行态队列。
    pub fn stop(&mut self) {
        self.stopped = true;
        self.scheduled_at = None;
        self.scheduled_delay_ms = None;
        self.scheduled_kind = None;
        self.debounce_started_at = None;
        self.last_sync_finished_at = None;
        self.last_poll_at = None;
        self.poll_snapshot.clear();

        if let Some(mut watcher) = self.recursive_watcher.take() {
            watcher.close();
        }
        for watcher in self.dir_watchers.values_mut() {
            watcher.close();
        }
        self.dir_watchers.clear();
        self.dir_cap_warned = false;
        self.inotify_limit_warned = false;
        self.lock_retry_count = 0;
        self.inert = false;

        self.pending_files.clear();
        self.ready = false;
        self.ignore_matcher = None;
        if is_test_runtime() {
            self.unregister_live_watcher_for_tests();
        }
        log_debug("File watcher stopped", None);
    }

    /// 测试入口：让合成的 project-relative 变更走真实事件同一条过滤和 debounce 路径。
    pub fn ingest_event_for_tests(&mut self, rel_path: &str) {
        self.handle_change(&normalize_path(rel_path));
    }

    /// 当前是否有 watcher 状态机处于活动状态。
    pub fn is_active(&self) -> bool {
        (self.recursive_watcher.is_some() || !self.dir_watchers.is_empty() || self.inert)
            && !self.stopped
    }

    /// live watching 是否已永久降级。
    pub fn is_degraded(&self) -> bool {
        self.degraded_reason.is_some()
    }

    /// live watching 的降级原因；健康时为 `None`。
    pub fn get_degraded_reason(&self) -> Option<&str> {
        self.degraded_reason.as_deref()
    }

    /// 等待 watch 集合安装完成；主要给测试和 MCP 首次工具调用的 catch-up gate 使用。
    pub fn wait_until_ready(&self, timeout_ms: u64) -> Result<(), String> {
        if self.ready {
            return Ok(());
        }
        let start = Instant::now();
        let timeout = Duration::from_millis(timeout_ms);
        while start.elapsed() < timeout {
            if self.ready {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        Err(format!(
            "FileWatcher.waitUntilReady timed out after {timeout_ms}ms"
        ))
    }

    /// 自上次成功同步以来 watcher 看到的文件快照。
    pub fn get_pending_files(&self) -> Vec<PendingFile> {
        self.pending_files
            .iter()
            .map(|(file_path, info)| PendingFile {
                path: file_path.clone(),
                first_seen_ms: info.first_seen_ms,
                last_seen_ms: info.last_seen_ms,
                // 如果当前 sync 开始时间晚于文件最后事件，该文件正在被本轮索引覆盖。
                indexing: self.syncing && self.sync_started_ms >= info.last_seen_ms,
            })
            .collect()
    }

    /// 运行时定时器由宿主线程驱动；到达记录的 debounce/backoff 延迟后执行一次 flush。
    pub fn flush_due(&mut self) {
        let Some(started_at) = self.scheduled_at else {
            return;
        };
        let Some(delay_ms) = self.scheduled_delay_ms else {
            return;
        };
        if started_at.elapsed() >= Duration::from_millis(delay_ms) {
            // 背靠背节流：debounce 已到期，但若距上次重型同步完成还不够最小间隔，
            // 就把这次 flush 推迟到间隔满足时，而不是紧贴着再跑一轮重型同步。
            // 间隔有上限（见 MAX_MIN_SYNC_INTERVAL_MS），pending 不会被永久饿死。
            if self.scheduled_kind == Some(super::types::ScheduledSyncKind::Debounce) {
                if let Some(remaining_ms) = self.throttle_remaining_ms() {
                    self.scheduled_at = Some(Instant::now());
                    self.scheduled_delay_ms = Some(remaining_ms);
                    self.scheduled_kind = Some(super::types::ScheduledSyncKind::Debounce);
                    return;
                }
            }
            self.scheduled_at = None;
            self.scheduled_delay_ms = None;
            self.scheduled_kind = None;
            self.flush();
        }
    }

    /// drain 原生事件并执行到期的 debounce/backoff 工作；运行时宿主从小型驱动线程调用。
    pub fn tick(&mut self) {
        self.drain_watch_events();
        self.poll_for_changes();
        self.flush_due();
    }

    /// 立即执行一次 flush，对应 TypeScript 版本里的私有 async `flush()`。
    pub fn flush_now(&mut self) {
        self.scheduled_at = None;
        self.scheduled_delay_ms = None;
        self.scheduled_kind = None;
        self.flush();
    }

    /// 测试专用：读取当前记录的 debounce/backoff 延迟。
    #[doc(hidden)]
    pub fn __scheduled_delay_ms_for_tests(&self) -> Option<u64> {
        self.scheduled_delay_ms
    }
}

impl Drop for FileWatcher {
    fn drop(&mut self) {
        // 防御性关闭，确保测试失败或调用方忘记 unwatch 时也释放 OS watch 句柄。
        if self.is_active() || self.ready {
            self.stop();
        }
    }
}
