//! Git sync hook management translated from `git-hooks.ts`.
//!
//! 中文维护提示：安装/卸载必须只触碰 RustCodeGraph 管理块，保留用户已有 hook
//! 内容；marker 是往返幂等的边界。

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const MARKER_BEGIN: &str = "# >>> rustcodegraph sync hook >>>";
const MARKER_END: &str = "# <<< rustcodegraph sync hook <<<";

/// Git hooks installed by default: commit, merge (`git pull`), and checkout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum GitHookName {
    PostCommit,
    PostMerge,
    PostCheckout,
}

impl GitHookName {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PostCommit => "post-commit",
            Self::PostMerge => "post-merge",
            Self::PostCheckout => "post-checkout",
        }
    }
}

pub const DEFAULT_SYNC_HOOKS: [GitHookName; 3] = [
    GitHookName::PostCommit,
    GitHookName::PostMerge,
    GitHookName::PostCheckout,
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitHookResult {
    /// Hook names that were created, updated, or removed.
    pub installed: Vec<GitHookName>,
    /// Resolved hooks directory, or `None` when not a git repo.
    pub hooks_dir: Option<String>,
    /// Reason nothing happened.
    pub skipped: Option<String>,
}

/// Whether `project_root` is inside a git working tree.
pub fn is_git_repo(project_root: impl AsRef<Path>) -> bool {
    let output = Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .current_dir(project_root)
        .output();
    let Ok(output) = output else {
        return false;
    };
    output.status.success() && String::from_utf8_lossy(&output.stdout).trim() == "true"
}

/// Install or update CodeGraph sync hooks in a git repository.
///
/// Pass `None` for `hooks` to use `DEFAULT_SYNC_HOOKS`.
pub fn install_git_sync_hook(
    project_root: impl AsRef<Path>,
    hooks: Option<&[GitHookName]>,
) -> GitHookResult {
    let project_root = project_root.as_ref();
    let Some(hooks_dir) = git_hooks_dir(project_root) else {
        return skipped(None, "not a git repository");
    };

    if fs::create_dir_all(&hooks_dir).is_err() {
        return skipped(
            Some(path_string(&hooks_dir)),
            "could not access the git hooks directory",
        );
    }

    let block = marker_block();
    let hooks = hooks.unwrap_or(&DEFAULT_SYNC_HOOKS);
    let mut installed = Vec::new();

    for hook in hooks {
        let file = hooks_dir.join(hook.as_str());
        let content = if file.exists() {
            let existing = fs::read_to_string(&file).unwrap_or_default();
            let base = strip_trailing_whitespace(&strip_marker_block(&existing));
            // 先剥掉旧管理块再追加新块，保证重复 install 字节稳定且不会叠块。
            if base.is_empty() {
                format!("#!/bin/sh\n{block}\n")
            } else {
                format!("{base}\n\n{block}\n")
            }
        } else {
            format!("#!/bin/sh\n{block}\n")
        };

        if fs::write(&file, content).is_ok() {
            chmod_executable(&file);
            installed.push(*hook);
        }
    }

    GitHookResult {
        installed,
        hooks_dir: Some(path_string(&hooks_dir)),
        skipped: None,
    }
}

/// Remove CodeGraph sync hooks, preserving any user-authored hook content.
///
/// Pass `None` for `hooks` to use `DEFAULT_SYNC_HOOKS`.
pub fn remove_git_sync_hook(
    project_root: impl AsRef<Path>,
    hooks: Option<&[GitHookName]>,
) -> GitHookResult {
    let project_root = project_root.as_ref();
    let Some(hooks_dir) = git_hooks_dir(project_root) else {
        return skipped(None, "not a git repository");
    };

    let hooks = hooks.unwrap_or(&DEFAULT_SYNC_HOOKS);
    let mut removed = Vec::new();

    for hook in hooks {
        let file = hooks_dir.join(hook.as_str());
        let Ok(original) = fs::read_to_string(&file) else {
            continue;
        };
        if !original.contains(MARKER_BEGIN) {
            continue;
        }

        let stripped = strip_marker_block(&original);
        let changed = if is_effectively_empty(&stripped) {
            fs::remove_file(&file).is_ok()
        } else {
            let body = format!("{}\n", strip_trailing_whitespace(&stripped));
            let ok = fs::write(&file, body).is_ok();
            if ok {
                chmod_executable(&file);
            }
            ok
        };
        if changed {
            removed.push(*hook);
        }
    }

    GitHookResult {
        installed: removed,
        hooks_dir: Some(path_string(&hooks_dir)),
        skipped: None,
    }
}

/// Whether any CodeGraph sync hook is currently installed.
///
/// Pass `None` for `hooks` to use `DEFAULT_SYNC_HOOKS`.
pub fn is_sync_hook_installed(
    project_root: impl AsRef<Path>,
    hooks: Option<&[GitHookName]>,
) -> bool {
    let Some(hooks_dir) = git_hooks_dir(project_root.as_ref()) else {
        return false;
    };
    hooks.unwrap_or(&DEFAULT_SYNC_HOOKS).iter().any(|hook| {
        let file = hooks_dir.join(hook.as_str());
        fs::read_to_string(file)
            .map(|content| content.contains(MARKER_BEGIN))
            .unwrap_or(false)
    })
}

fn git_hooks_dir(project_root: &Path) -> Option<PathBuf> {
    // 使用 `git rev-parse --git-path hooks` 兼容 worktree、submodule 和自定义 git dir。
    let output = Command::new("git")
        .args(["rev-parse", "--git-path", "hooks"])
        .current_dir(project_root)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let out = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if out.is_empty() {
        return None;
    }
    let path = PathBuf::from(out);
    Some(if path.is_absolute() {
        path
    } else {
        absolutize(project_root).join(path)
    })
}

fn marker_block() -> String {
    [
        MARKER_BEGIN,
        "# Keeps the CodeGraph index fresh while the live file watcher is off",
        "# (e.g. WSL2 /mnt drives). Runs in the background so it never blocks git.",
        "# Managed by RustCodeGraph; remove with `rustcodegraph uninit` or delete this block.",
        "if command -v rustcodegraph >/dev/null 2>&1; then",
        "  ( rustcodegraph sync >/dev/null 2>&1 & ) >/dev/null 2>&1",
        "fi",
        MARKER_END,
    ]
    .join("\n")
}

fn strip_marker_block(content: &str) -> String {
    // marker 块之外的所有行原样保留；即便用户 hook 中有注释或 shell 逻辑也不改写。
    let mut kept = Vec::new();
    let mut in_block = false;
    for line in content.split('\n') {
        let trimmed = line.trim();
        if trimmed == MARKER_BEGIN {
            in_block = true;
            continue;
        }
        if trimmed == MARKER_END {
            in_block = false;
            continue;
        }
        if !in_block {
            kept.push(line);
        }
    }
    kept.join("\n")
}

fn is_effectively_empty(content: &str) -> bool {
    content.lines().all(|line| {
        let trimmed = line.trim();
        trimmed.is_empty() || trimmed.starts_with("#!")
    })
}

fn strip_trailing_whitespace(content: &str) -> String {
    content.trim_end().to_owned()
}

fn skipped(hooks_dir: Option<String>, reason: &str) -> GitHookResult {
    GitHookResult {
        installed: Vec::new(),
        hooks_dir,
        skipped: Some(reason.to_owned()),
    }
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn absolutize(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    }
}

#[cfg(unix)]
fn chmod_executable(file: &Path) {
    // POSIX hook 必须可执行；Windows 下 Git 不依赖该 mode，因此非 Unix 分支是 no-op。
    use std::os::unix::fs::PermissionsExt;

    if let Ok(metadata) = fs::metadata(file) {
        let mut permissions = metadata.permissions();
        permissions.set_mode(0o755);
        let _ = fs::set_permissions(file, permissions);
    }
}

#[cfg(not(unix))]
fn chmod_executable(_file: &Path) {}
