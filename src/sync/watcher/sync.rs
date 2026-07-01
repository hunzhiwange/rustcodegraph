//! watcher 触发同步、退避重试和降级处理。
//!
//! 文件事件只负责把路径放进 pending 队列；这里决定何时调用 sync_fn、如何处理
//! 写锁竞争，以及哪些错误需要把 live watching 永久关闭。

use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde_json::json;

use crate::errors::{ErrorContext, log_debug, log_warn};

use super::backend::WatchStartError;
use super::state::FileWatcher;
use super::types::{
    EXHAUSTION_REASON, INOTIFY_LIMIT_REASON, MAX_LOCK_RETRIES, MAX_LOCK_RETRY_DELAY_MS,
    ScheduledSyncKind, WatchSyncError,
};
use super::util::{context, now_ms, path_string};

impl FileWatcher {
    pub(super) fn degrade(&mut self, reason: &str, mut context: ErrorContext) {
        if self.degraded_reason.is_some() {
            return;
        }
        // 降级是一次性状态：通知调用方后立即 stop，避免半健康 watcher 继续制造
        // stale 状态或重复告警。
        self.degraded_reason = Some(reason.to_owned());
        context.insert(
            "projectRoot".to_owned(),
            json!(path_string(&self.project_root)),
        );
        context.insert("reason".to_owned(), json!(reason));
        log_warn("File watcher disabled", Some(&context));
        if let Some(callback) = &self.on_degraded {
            callback(reason);
        }
        self.stop();
    }

    pub(super) fn warn_inotify_limit(&mut self, mut context: ErrorContext) {
        if self.inotify_limit_warned {
            return;
        }
        // inotify 配额耗尽不等同于所有 watch 失效；保留已覆盖目录，但停止继续扩张。
        self.inotify_limit_warned = true;
        context.insert("watchedDirs".to_owned(), json!(self.dir_watchers.len()));
        log_warn(INOTIFY_LIMIT_REASON, Some(&context));
    }

    pub(super) fn schedule_sync(&mut self) {
        if self.scheduled_kind == Some(ScheduledSyncKind::Retry) {
            return;
        }

        let now = Instant::now();
        // 本轮 debounce 周期的第一个事件确定 max-wait 的计时起点；后续事件不重置它。
        let epoch_start = *self.debounce_started_at.get_or_insert(now);

        // 新事件正常会把 debounce 起点推后到现在（等写入稳定）。但 debounce 不能超过
        // 从本轮第一个事件起的 max-wait 上限：持续事件流（复制大文件夹）下，纯重置会
        // 让 sync 永不触发；这里把本次 flush 钳到不晚于 max-wait 截止时刻。
        let deadline = epoch_start + Duration::from_millis(self.max_debounce_ms);
        let remaining_to_deadline = deadline.saturating_duration_since(now).as_millis() as u64;
        let delay = self.debounce_ms.min(remaining_to_deadline);

        self.scheduled_at = Some(now);
        self.scheduled_delay_ms = Some(delay);
        self.scheduled_kind = Some(ScheduledSyncKind::Debounce);
    }

    fn schedule_retry_sync(&mut self, delay_ms: u64) {
        // 锁竞争/可恢复跳过的重试使用单独延迟，不改变 pending 文件集合。
        self.scheduled_at = Some(Instant::now());
        self.scheduled_delay_ms = Some(delay_ms);
        self.scheduled_kind = Some(ScheduledSyncKind::Retry);
    }

    fn skipped_sync_retry_delay_ms(&self) -> u64 {
        self.debounce_ms
            .max(self.max_debounce_ms)
            .max(self.min_sync_interval_ms)
    }

    /// 距上次重型同步完成还需等待多久才允许下一轮（背靠背节流）。
    ///
    /// 返回 `Some(剩余毫秒)` 表示当前处于最小间隔内、应推迟；`None` 表示可以放行
    /// （从未跑过、间隔为 0、或间隔已过）。这只约束自动 debounce 路径，`flush_now`
    /// 的强制刷新不受它影响。
    pub(super) fn throttle_remaining_ms(&self) -> Option<u64> {
        if self.min_sync_interval_ms == 0 {
            return None;
        }
        let elapsed = self.last_sync_finished_at?.elapsed();
        let interval = Duration::from_millis(self.min_sync_interval_ms);
        if elapsed >= interval {
            return None;
        }
        // 至少等 1ms，避免 0 延迟时定时器立刻又判到期形成忙环。
        Some(((interval - elapsed).as_millis() as u64).max(1))
    }

    pub(super) fn flush(&mut self) {
        // sync_fn 可能访问 SQLite 和文件系统；禁止重入，stop 后也不再追赶。
        if self.syncing || self.stopped {
            return;
        }

        self.sync_started_ms = now_ms();
        self.syncing = true;
        // 本轮 debounce 周期到此结束：清空 max-wait 计时起点，之后到来的事件会通过
        // schedule_sync 开启新一轮（而不是沿用旧 epoch 立刻被判过期）。
        self.debounce_started_at = None;

        let result = (self.sync_fn)();
        let mut skipped_sync_retry_delay_ms = None;
        match result {
            Ok(result) if result.skipped => {
                // 可恢复跳过：本轮没做重型同步。**不**清理 pending，也不当作正常完成
                // 回调，保持 watcher active。下一轮使用 retry 调度，避免
                // skip→debounce→skip 短周期空转。
                self.lock_retry_count = 0;
                self.last_sync_finished_at = Some(Instant::now());
                skipped_sync_retry_delay_ms = Some(self.skipped_sync_retry_delay_ms());
                let ctx = context([
                    ("pendingFiles", json!(self.pending_files.len())),
                    ("filesChanged", json!(result.files_changed)),
                ]);
                log_warn(
                    "Watch sync skipped by the sync callback; keeping pending and retrying later.",
                    Some(&ctx),
                );
            }
            Ok(result) => {
                self.lock_retry_count = 0;
                self.last_sync_finished_at = Some(Instant::now());
                let sync_started_ms = self.sync_started_ms;
                // 清掉本轮同步开始前已经看到的事件；同步期间新来的事件保留到下一轮。
                self.pending_files
                    .retain(|_, info| info.last_seen_ms > sync_started_ms);
                if let Some(callback) = &self.on_sync_complete {
                    callback(&result);
                }
            }
            Err(WatchSyncError::LockUnavailable(err)) => {
                self.lock_retry_count += 1;
                // 另一个进程持有写锁是预期并发情况：先退避重试，超过预算再降级。
                let ctx = context([
                    ("pendingFiles", json!(self.pending_files.len())),
                    ("retryCount", json!(self.lock_retry_count)),
                ]);
                log_debug("Watch sync skipped: file lock unavailable", Some(&ctx));
                if self.lock_retry_count > MAX_LOCK_RETRIES {
                    self.degrade(
                        "RustCodeGraph file lock held by another process past the retry budget; \
                         auto-sync disabled. Run `rustcodegraph sync` once the other writer finishes \
                         (or install git sync hooks) to refresh the graph.",
                        context([
                            ("pendingFiles", json!(self.pending_files.len())),
                            ("retryCount", json!(self.lock_retry_count)),
                            ("error", json!(err.message())),
                        ]),
                    );
                }
            }
            Err(err) => {
                self.lock_retry_count = 0;
                // 普通同步错误不清空 pending，让下一轮事件或重试仍有机会追赶索引。
                let ctx = context([("error", json!(err.message()))]);
                log_warn("Watch sync failed", Some(&ctx));
                if let Some(callback) = &self.on_sync_error {
                    callback(&err);
                }
            }
        }

        self.syncing = false;
        if !self.pending_files.is_empty() && !self.stopped {
            if self.lock_retry_count > 0 {
                // 指数退避有上限，避免长时间锁竞争时忙等，也避免永久沉默。
                let exponent = self.lock_retry_count.saturating_sub(1).min(31);
                let multiplier = 1u64 << exponent;
                let retry_delay_ms = self
                    .debounce_ms
                    .saturating_mul(multiplier)
                    .min(MAX_LOCK_RETRY_DELAY_MS);
                self.schedule_retry_sync(retry_delay_ms);
            } else if let Some(retry_delay_ms) = skipped_sync_retry_delay_ms {
                self.schedule_retry_sync(retry_delay_ms);
            } else {
                self.schedule_sync();
            }
        }
    }

    pub(super) fn handle_start_error(&mut self, err: WatchStartError) {
        // 启动阶段只有资源耗尽会写入永久降级原因；其它错误只表示 watcher 没启动。
        match err {
            WatchStartError::ResourceExhaustion(message) => {
                self.degrade(EXHAUSTION_REASON, context([("error", json!(message))]));
            }
            other => {
                let ctx = context([("error", json!(other.message()))]);
                log_warn("Could not start file watcher", Some(&ctx));
                self.stop();
            }
        }
    }

    pub(super) fn handle_runtime_watch_error(
        &mut self,
        err: WatchStartError,
        dir: Option<PathBuf>,
    ) {
        // 运行期错误按影响面处理：全局资源耗尽降级，inotify 限额摘掉目录分支，
        // 其它目录错误只记录并关闭对应 watch。
        match err {
            WatchStartError::ResourceExhaustion(message) => {
                let mut ctx = context([("error", json!(message))]);
                if let Some(dir) = &dir {
                    ctx.insert("dir".to_owned(), json!(path_string(dir)));
                }
                self.degrade(EXHAUSTION_REASON, ctx);
            }
            WatchStartError::InotifyExhaustion(message) => {
                let mut ctx = context([("error", json!(message))]);
                if let Some(dir) = &dir {
                    ctx.insert("dir".to_owned(), json!(path_string(dir)));
                }
                self.warn_inotify_limit(ctx);
                if let Some(dir) = dir {
                    self.unwatch_dir(&dir);
                }
            }
            WatchStartError::Other(message) => {
                let mut ctx = context([("error", json!(message))]);
                if let Some(dir) = &dir {
                    ctx.insert("dir".to_owned(), json!(path_string(dir)));
                    self.unwatch_dir(dir);
                }
                log_warn("File watcher error", Some(&ctx));
            }
        }
    }
}
