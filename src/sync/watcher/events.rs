//! 文件系统事件接入和过滤。
//!
//! 这里把 OS watcher 的路径事件转换成 project-relative 源文件变更，并负责
//! 逐目录监听时的新子目录补挂载。真正的同步调度在 `sync.rs` 中完成。

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, UNIX_EPOCH};

use serde_json::json;

use crate::directory::is_code_graph_data_dir;
use crate::errors::{log_debug, log_warn};
use crate::extraction::grammars::is_source_file;

use super::backend::{WatchStartError, create_watch_handle, max_dir_watches};
use super::state::FileWatcher;
use super::types::{EXHAUSTION_REASON, FileSnapshot, PendingInfo};
use super::util::{context, now_ms, path_string, relative_posix};

impl FileWatcher {
    pub(super) fn start_recursive(&mut self) -> Result<(), WatchStartError> {
        let mut handle = create_watch_handle(&self.project_root, true)?;
        let watcher_ptr = self as *mut Self as usize;
        handle.set_error_handler(Box::new(move |err| {
            // 错误回调从底层 handle 反向通知状态机；handle 生命周期由 FileWatcher
            // 持有，stop/drop 会先关闭它，再释放 watcher 状态。
            let watcher = unsafe { (watcher_ptr as *mut FileWatcher).as_mut() };
            if let Some(watcher) = watcher {
                watcher.handle_runtime_watch_error(err, None);
            }
        }));
        self.recursive_watcher = Some(handle);
        Ok(())
    }

    pub(super) fn start_per_directory(&mut self) {
        // Linux 不使用递归 watch：逐目录挂载可以在配额耗尽时精确知道覆盖范围。
        let root = self.project_root.clone();
        self.watch_tree(&root, false);
    }

    pub(super) fn watch_tree(&mut self, dir: &Path, mark_existing: bool) {
        // 一旦 watcher 已永久降级或 inotify 配额已告警，就不继续扩张 watch 集合，
        // 避免后续目录扫描制造更多部分覆盖状态。
        if self.stopped || self.degraded_reason.is_some() || self.inotify_limit_warned {
            return;
        }
        if self.dir_watchers.contains_key(dir) {
            return;
        }
        if self.dir_watchers.len() >= max_dir_watches() {
            if !self.dir_cap_warned {
                self.dir_cap_warned = true;
                // cap 是进程内保护，不代表 OS 已失败；保留已有 watch，未覆盖子树
                // 依赖手动/周期性 sync。
                let ctx = context([("cap", json!(max_dir_watches()))]);
                log_warn(
                    "File watcher hit directory-watch cap; remaining subtrees rely on manual/periodic sync",
                    Some(&ctx),
                );
            }
            return;
        }

        match create_watch_handle(dir, false) {
            Ok(mut handle) => {
                let watcher_ptr = self as *mut Self as usize;
                let watched_dir = dir.to_path_buf();
                handle.set_error_handler(Box::new(move |err| {
                    // 逐目录模式需要携带出错目录，运行时才能只摘掉失败分支。
                    let watcher = unsafe { (watcher_ptr as *mut FileWatcher).as_mut() };
                    if let Some(watcher) = watcher {
                        watcher.handle_runtime_watch_error(err, Some(watched_dir.clone()));
                    }
                }));
                self.dir_watchers.insert(dir.to_path_buf(), handle);
            }
            Err(WatchStartError::ResourceExhaustion(message)) => {
                // 文件描述符/句柄耗尽会影响整个 watcher，继续尝试只会刷屏。
                self.degrade(
                    EXHAUSTION_REASON,
                    context([("error", json!(message)), ("dir", json!(path_string(dir)))]),
                );
                return;
            }
            Err(WatchStartError::InotifyExhaustion(message)) => {
                // inotify watch 数耗尽时已有目录仍可能有效；告警后停止扩张覆盖范围。
                self.warn_inotify_limit(context([
                    ("error", json!(message)),
                    ("dir", json!(path_string(dir))),
                ]));
                return;
            }
            Err(WatchStartError::Other(_)) => return,
        }

        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_dir() {
                if !self.should_ignore_dir(&path) {
                    self.watch_tree(&path, mark_existing);
                }
            } else if mark_existing && file_type.is_file() {
                // 新挂载目录里已经存在的源文件需要入 pending，否则新建目录后的文件
                // 可能等不到单独的文件事件。
                let rel = relative_posix(&self.project_root, &path);
                self.handle_change(&rel);
            }
        }
    }

    fn handle_directory_event(&mut self, dir: &Path) {
        if self.should_ignore_dir(dir) {
            return;
        }
        if self.recursive_watcher.is_some() {
            // macOS/Windows 的递归 watcher 已经覆盖新目录。这里绝不能再调用
            // watch_tree，否则每次目录事件都会额外创建一棵逐目录 watcher。
            // 只补记本轮 watcher 快照里尚不存在的源码，保留整目录复制的同步语义。
            self.mark_new_source_files(dir);
        } else {
            // Linux 逐目录模式必须为新目录及其子目录补挂 watch。
            self.watch_tree(dir, true);
        }
    }

    fn mark_new_source_files(&mut self, dir: &Path) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_dir() {
                if !self.should_ignore_dir(&path) {
                    self.mark_new_source_files(&path);
                }
                continue;
            }

            let rel = relative_posix(&self.project_root, &path);
            if !self.poll_snapshot.contains_key(&rel) {
                self.handle_change(&rel);
            }
        }
    }

    pub(super) fn drain_watch_events(&mut self) {
        // 先 drain 再处理，避免持有底层队列锁时递归挂载新目录或调度同步。
        let recursive_events = self
            .recursive_watcher
            .as_mut()
            .map(|watcher| watcher.take_events())
            .unwrap_or_default();
        for path in recursive_events {
            self.handle_path_event(None, &path);
        }

        let dirs = self.dir_watchers.keys().cloned().collect::<Vec<_>>();
        for dir in dirs {
            let events = self
                .dir_watchers
                .get_mut(&dir)
                .map(|watcher| watcher.take_events())
                .unwrap_or_default();
            for path in events {
                self.handle_path_event(Some(&dir), &path);
            }
        }
    }

    pub(super) fn poll_for_changes(&mut self) {
        if self.stopped || !self.ready || self.inert {
            return;
        }
        let now = Instant::now();
        if self
            .last_poll_at
            .is_some_and(|last| last.elapsed() < Duration::from_millis(self.poll_interval_ms))
        {
            return;
        }
        self.last_poll_at = Some(now);

        let previous = std::mem::take(&mut self.poll_snapshot);
        let current = self.collect_source_snapshot();
        for (rel, snapshot) in &current {
            if previous.get(rel) != Some(snapshot) {
                self.handle_change(rel);
            }
        }
        for rel in previous.keys() {
            if !current.contains_key(rel) {
                self.handle_deleted_or_unknown_path(rel);
            }
        }
        self.poll_snapshot = current;
    }

    pub(super) fn collect_source_snapshot(&self) -> BTreeMap<String, FileSnapshot> {
        let mut snapshot = BTreeMap::new();
        let mut visited_dirs = HashSet::new();
        self.collect_source_snapshot_dir(&self.project_root, &mut visited_dirs, &mut snapshot);
        snapshot
    }

    fn collect_source_snapshot_dir(
        &self,
        dir: &Path,
        visited_dirs: &mut HashSet<PathBuf>,
        snapshot: &mut BTreeMap<String, FileSnapshot>,
    ) {
        let Ok(canonical) = fs::canonicalize(dir) else {
            return;
        };
        if !visited_dirs.insert(canonical) {
            return;
        }

        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_dir() {
                if !self.should_ignore_dir(&path) {
                    self.collect_source_snapshot_dir(&path, visited_dirs, snapshot);
                }
                continue;
            }

            let rel = relative_posix(&self.project_root, &path);
            if rel.is_empty()
                || rel == "."
                || rel.starts_with("..")
                || self.is_always_ignored(&rel)
                || self
                    .ignore_matcher
                    .as_ref()
                    .is_some_and(|matcher| matcher.ignores(&rel))
                || !is_source_file(&rel)
            {
                continue;
            }

            let metadata = if file_type.is_symlink() {
                fs::metadata(&path)
            } else {
                entry.metadata()
            };
            let Ok(metadata) = metadata else {
                continue;
            };
            if !metadata.is_file() {
                continue;
            }
            let modified_ns = metadata
                .modified()
                .ok()
                .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
                .map(|duration| duration.as_nanos())
                .unwrap_or(0);
            snapshot.insert(
                rel,
                FileSnapshot {
                    modified_ns,
                    len: metadata.len(),
                },
            );
        }
    }

    #[allow(dead_code)]
    fn handle_dir_event(&mut self, dir: &Path, filename: &str) {
        // 保留给更窄的目录事件 backend：语义与 handle_path_event 一致。
        if self.stopped || filename.is_empty() {
            return;
        }
        let full = dir.join(filename);
        if fs::metadata(&full)
            .map(|metadata| metadata.is_dir())
            .unwrap_or(false)
        {
            self.handle_directory_event(&full);
            return;
        }

        let rel = relative_posix(&self.project_root, &full);
        self.handle_change(&rel);
    }

    fn handle_path_event(&mut self, watched_dir: Option<&Path>, path: &Path) {
        if self.stopped {
            return;
        }
        // notify 在不同平台上可能给绝对路径，也可能给相对被监听目录的路径。
        let full = if path.is_absolute() {
            path.to_path_buf()
        } else if let Some(dir) = watched_dir {
            dir.join(path)
        } else {
            self.project_root.join(path)
        };
        let rel = relative_posix(&self.project_root, &full);
        match fs::metadata(&full) {
            Ok(metadata) if metadata.is_dir() => {
                self.handle_directory_event(&full);
            }
            Ok(_) => self.handle_change(&rel),
            Err(_) if is_source_file(&rel) => self.handle_change(&rel),
            Err(_) => {
                self.unwatch_deleted_subtree(&full);
                self.handle_deleted_or_unknown_path(&rel);
            }
        }
    }

    pub(super) fn handle_change(&mut self, rel: &str) {
        // 所有入口最终都会经过同一组过滤，确保测试事件、递归事件和逐目录事件
        // 对 .git、.rustcodegraph、ignore 规则及语言支持的判断一致。
        if rel.is_empty() || rel == "." || rel.starts_with("..") {
            return;
        }
        if self.is_always_ignored(rel) {
            return;
        }
        if self
            .ignore_matcher
            .as_ref()
            .is_some_and(|matcher| matcher.ignores(rel))
        {
            return;
        }
        if !is_source_file(rel) {
            return;
        }

        let ctx = context([("file", json!(rel))]);
        log_debug("File change detected", Some(&ctx));
        if self.ready {
            let now = now_ms();
            let first_seen_ms = self
                .pending_files
                .get(rel)
                .map(|info| info.first_seen_ms)
                .unwrap_or(now);
            // 多次事件保留首次出现时间，用于 stale 提示；同步清理则看最后一次事件。
            self.pending_files.insert(
                rel.to_owned(),
                PendingInfo {
                    first_seen_ms,
                    last_seen_ms: now,
                    indexing: false,
                },
            );
        }
        self.schedule_sync();
    }

    fn handle_deleted_or_unknown_path(&mut self, rel: &str) {
        // Directory removals often arrive as a single path event for the deleted
        // directory, not as one delete event per contained source file. The
        // incremental sync can discover removed indexed files by scanning the
        // current tree, so this path only needs to schedule a batch.
        if rel.is_empty() || rel == "." || rel.starts_with("..") {
            return;
        }
        if self.is_always_ignored(rel) {
            return;
        }
        let ignored = self
            .ignore_matcher
            .as_ref()
            .is_some_and(|matcher| matcher.ignores(rel) || matcher.ignores(&format!("{rel}/")));
        if ignored {
            return;
        }

        let ctx = context([("path", json!(rel))]);
        log_debug(
            "Deleted or unknown path detected; scheduling sync",
            Some(&ctx),
        );
        if self.ready {
            let now = now_ms();
            let first_seen_ms = self
                .pending_files
                .get(rel)
                .map(|info| info.first_seen_ms)
                .unwrap_or(now);
            self.pending_files.insert(
                rel.to_owned(),
                PendingInfo {
                    first_seen_ms,
                    last_seen_ms: now,
                    indexing: false,
                },
            );
        }
        self.schedule_sync();
    }

    fn unwatch_deleted_subtree(&mut self, deleted: &Path) {
        let watched_dirs = self
            .dir_watchers
            .keys()
            .filter(|dir| dir.as_path() == deleted || dir.starts_with(deleted))
            .cloned()
            .collect::<Vec<_>>();
        for dir in watched_dirs {
            self.unwatch_dir(&dir);
        }
    }

    pub(super) fn unwatch_dir(&mut self, dir: &Path) {
        if let Some(mut watcher) = self.dir_watchers.remove(dir) {
            watcher.close();
        }
    }

    fn is_always_ignored(&self, rel: &str) -> bool {
        let top = rel.split('/').next().unwrap_or(rel);
        // 索引目录和 git 元数据变化不应触发自我同步循环。
        is_code_graph_data_dir(top) || rel == ".git" || rel.starts_with(".git/")
    }

    fn should_ignore_dir(&self, dir_path: &Path) -> bool {
        let rel = relative_posix(&self.project_root, dir_path);
        if rel.is_empty() || rel == "." || rel.starts_with("..") {
            return false;
        }
        if self.is_always_ignored(&rel) {
            return true;
        }
        // 目录匹配带尾斜杠，复用扫描阶段的 ignore 语义。
        self.ignore_matcher
            .as_ref()
            .map(|matcher| matcher.ignores(&(rel + "/")))
            .unwrap_or(false)
    }
}
