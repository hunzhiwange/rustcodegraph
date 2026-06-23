//! Sync module facade translated from `index.ts`.
//!
//! 中文维护提示：这是同步子系统对外 re-export 的稳定入口；调用方不应依赖内部
//! watcher/backend 子模块路径。

pub use super::git_hooks::{
    DEFAULT_SYNC_HOOKS, GitHookName, GitHookResult, install_git_sync_hook, is_git_repo,
    is_sync_hook_installed, remove_git_sync_hook,
};
pub use super::watch_policy::{WatchProbe, detect_wsl, watch_disabled_reason};
pub use super::watcher::{
    FileWatcher, LockUnavailableError, PendingFile, SyncRunResult, WatchOptions, WatchStartError,
    WatchSyncError,
};
pub use super::worktree::{
    WorktreeIndexMismatch, detect_worktree_index_mismatch, git_worktree_root,
    worktree_mismatch_notice, worktree_mismatch_warning,
};
