//! 文件监听 facade。
//!
//! Runtime watcher 的状态会同步写入 facade runtime registry，让同进程 API 和外部 status 查询看到一致的
//! active/degraded/pending 文件视图。

use super::*;

impl CodeGraph {
    pub fn watch(&mut self, options: WatchOptions) -> bool {
        if self.watching {
            return true;
        }

        let debounce_ms = options.debounce_ms.unwrap_or(2000);
        let max_debounce_ms = options.max_debounce_ms;
        let min_sync_interval_ms = options.min_sync_interval_ms;
        let root = self.project_root.clone();
        let sync_root = root.clone();
        let watcher = Arc::new(StdMutex::new(RuntimeFileWatcher::new(
            &root,
            move || {
                // watcher 回调只返回汇总结果，具体增量清理和重建仍交给 sync 管线。
                // 如果 sync 管线报告 recoverable skipped，这里转成 watcher 能识别的
                // skipped 标志，让 watcher 保留 pending、保持 active、稍后重试，而
                // 不是把跳过当成正常完成。
                let result = sync_facade_database(&sync_root, Instant::now());
                let files_changed =
                    result.files_added + result.files_modified + result.files_removed;
                Ok(SyncRunResult {
                    files_changed,
                    duration_ms: result.duration_ms,
                    skipped: result.memory_skipped,
                })
            },
            RuntimeWatchOptions {
                debounce_ms: Some(debounce_ms),
                max_debounce_ms,
                min_sync_interval_ms,
                ..RuntimeWatchOptions::default()
            },
        )));
        let (active, degraded_reason) = watcher
            .lock()
            .map(|mut watcher| {
                let active = watcher.start();
                (active, watcher.get_degraded_reason().map(str::to_owned))
            })
            .unwrap_or((false, None));
        self.watcher_degraded_reason = degraded_reason;
        register_facade_runtime_watcher(&self.project_root, debounce_ms);
        self.watch_registered = true;
        self.watching = active;
        if active {
            // RuntimeFileWatcher 需要周期性 tick 来刷新 pending/degraded 状态并执行 debounce 后的同步。
            self.start_watch_sync_thread(Arc::clone(&watcher), debounce_ms);
            self.watcher = Some(watcher);
        } else {
            update_facade_runtime_watcher(
                &self.project_root,
                false,
                self.watcher_degraded_reason.clone(),
                Vec::new(),
            );
        }
        active
    }

    pub fn unwatch(&mut self) {
        // 先通知线程退出，再 join，最后停止底层 watcher，避免关闭时仍有 tick 访问已释放状态。
        if let Some(stop) = self.watch_stop.take() {
            stop.store(true, Ordering::SeqCst);
        }
        if let Some(thread) = self.watch_thread.take() {
            let _ = thread.join();
        }
        if let Some(watcher) = self.watcher.take()
            && let Ok(mut watcher) = watcher.lock()
        {
            watcher.stop();
        }
        if self.watch_registered {
            unregister_facade_watcher(&self.project_root);
        }
        self.watch_registered = false;
        self.watching = false;
    }

    pub fn is_watching(&self) -> bool {
        if let Some(watcher) = &self.watcher {
            return watcher
                .lock()
                .map(|watcher| watcher.is_active())
                .unwrap_or(false);
        }
        self.watching
    }

    pub fn is_watcher_degraded(&self) -> bool {
        self.watcher
            .as_ref()
            .and_then(|watcher| watcher.lock().ok())
            .is_some_and(|watcher| watcher.is_degraded())
            || self.watcher_degraded_reason.is_some()
            || facade_degraded_reason(&self.project_root).is_some()
    }

    pub fn get_watcher_degraded_reason(&self) -> Option<String> {
        self.watcher
            .as_ref()
            .and_then(|watcher| watcher.lock().ok())
            .and_then(|watcher| watcher.get_degraded_reason().map(str::to_owned))
            .or_else(|| self.watcher_degraded_reason.clone())
            .or_else(|| facade_degraded_reason(&self.project_root))
    }

    pub fn get_pending_files(&self) -> Vec<PendingFile> {
        if let Some(watcher) = &self.watcher
            && let Ok(watcher) = watcher.lock()
        {
            let pending = watcher.get_pending_files();
            update_facade_runtime_watcher(
                &self.project_root,
                watcher.is_active(),
                watcher.get_degraded_reason().map(str::to_owned),
                pending.clone(),
            );
            if !pending.is_empty() {
                return pending
                    .into_iter()
                    .map(|pending| PendingFile {
                        path: pending.path,
                        first_seen_ms: pending.first_seen_ms,
                        last_seen_ms: pending.last_seen_ms,
                        indexing: pending.indexing,
                    })
                    .collect();
            }
        }
        let pending = facade_pending_files(&self.project_root);
        if pending.is_empty() {
            return self.pending_files.clone();
        }
        pending
            .into_iter()
            .map(|pending| PendingFile {
                path: pending.path,
                first_seen_ms: pending.first_seen_ms,
                last_seen_ms: pending.last_seen_ms,
                indexing: pending.indexing,
            })
            .collect()
    }

    pub fn wait_until_watcher_ready(&self, timeout_ms: Option<u64>) {
        if let Some(watcher) = &self.watcher
            && let Ok(watcher) = watcher.lock()
        {
            let _ = watcher.wait_until_ready(timeout_ms.unwrap_or(10_000));
        }
    }

    fn start_watch_sync_thread(
        &mut self,
        watcher: Arc<StdMutex<RuntimeFileWatcher>>,
        debounce_ms: u64,
    ) {
        let root = self.project_root.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        let poll_interval = Duration::from_millis(debounce_ms.clamp(50, 250));
        let handle = thread::spawn(move || {
            while !thread_stop.load(Ordering::SeqCst) {
                thread::sleep(poll_interval);
                if thread_stop.load(Ordering::SeqCst) {
                    break;
                }
                if let Ok(mut watcher) = watcher.lock() {
                    watcher.tick();
                    update_facade_runtime_watcher(
                        &root,
                        watcher.is_active(),
                        watcher.get_degraded_reason().map(str::to_owned),
                        watcher.get_pending_files(),
                    );
                }
            }
        });
        self.watch_stop = Some(stop);
        self.watch_thread = Some(handle);
    }
}
