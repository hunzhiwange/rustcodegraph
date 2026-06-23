//! watcher 触发同步、退避重试和降级处理。
//!
//! 文件事件只负责把路径放进 pending 队列；这里决定何时调用 sync_fn、如何处理
//! 写锁竞争，以及哪些错误需要把 live watching 永久关闭。

use std::path::PathBuf;
use std::time::Instant;

use serde_json::json;

use crate::errors::{ErrorContext, log_debug, log_warn};

use super::backend::WatchStartError;
use super::state::FileWatcher;
use super::types::{
    EXHAUSTION_REASON, INOTIFY_LIMIT_REASON, MAX_LOCK_RETRIES, MAX_LOCK_RETRY_DELAY_MS,
    WatchSyncError,
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
        // 新事件总是重置 debounce 起点，等文件写入稳定后再同步。
        self.scheduled_at = Some(Instant::now());
        self.scheduled_delay_ms = Some(self.debounce_ms);
    }

    fn schedule_retry_sync(&mut self, delay_ms: u64) {
        // 锁竞争重试使用单独延迟，不改变 pending 文件集合。
        self.scheduled_at = Some(Instant::now());
        self.scheduled_delay_ms = Some(delay_ms);
    }

    pub(super) fn flush(&mut self) {
        // sync_fn 可能访问 SQLite 和文件系统；禁止重入，stop 后也不再追赶。
        if self.syncing || self.stopped {
            return;
        }

        self.sync_started_ms = now_ms();
        self.syncing = true;

        let result = (self.sync_fn)();
        match result {
            Ok(result) => {
                self.lock_retry_count = 0;
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
