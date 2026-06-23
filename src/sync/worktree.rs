//! Git worktree 与索引根目录错配检测。
//!
//! 一些 agent 会在主 checkout 下创建嵌套 worktree；如果向上寻找 `.rustcodegraph`
//! 时拿到主 checkout 的索引，查询结果就会悄悄来自另一条分支。这里负责在工具
//! 输出前发现这种情况并给出明确提示。

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// 当前编辑的 working tree 与 `.rustcodegraph` 索引所属 working tree 不一致。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeIndexMismatch {
    /// 命令实际运行所在的 git working tree。
    pub worktree_root: String,
    /// 当前解析到的 `.rustcodegraph` 索引所属的另一个 working tree。
    pub index_root: String,
}

/// 返回 `dir` 所属 git working tree 的绝对、解符号链接后的根目录。
///
/// 不在 git repo 内或缺少 git 时返回 `None`，让上层保持宽容。
pub fn git_worktree_root(dir: impl AsRef<Path>) -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(dir)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let out = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if out.is_empty() {
        None
    } else {
        Some(realpath(out))
    }
}

/// 检测 `start_path` 与解析到的 CodeGraph 索引是否属于不同 git working tree。
pub fn detect_worktree_index_mismatch(
    start_path: impl AsRef<Path>,
    index_root: impl AsRef<Path>,
) -> Option<WorktreeIndexMismatch> {
    let worktree_root = git_worktree_root(start_path)?;
    let resolved_index_root = realpath(index_root);

    if worktree_root == resolved_index_root {
        return None;
    }

    // 只有 index_root 本身也是一个 git worktree 根时才提示；普通外部目录可能是
    // 用户显式指定的索引位置，不应误判成 worktree 错配。
    if git_worktree_root(&resolved_index_root).as_deref() != Some(resolved_index_root.as_str()) {
        return None;
    }

    Some(WorktreeIndexMismatch {
        worktree_root,
        index_root: resolved_index_root,
    })
}

/// 面向 status 输出的多行 warning，每行只承载一个事实。
pub fn worktree_mismatch_warning(m: &WorktreeIndexMismatch) -> String {
    format!(
        "This RustCodeGraph index belongs to a different git working tree.\n  Running in: {}\n  \
         Index from: {}\nResults reflect that tree's code (often a different branch), not this worktree — \
         symbols changed only here are missing. Run \"rustcodegraph init -i\" in this worktree \
         for a worktree-local index.",
        m.worktree_root, m.index_root
    )
}

/// 面向工具结果前缀的紧凑单行提示。
pub fn worktree_mismatch_notice(m: &WorktreeIndexMismatch) -> String {
    format!(
        "⚠ RustCodeGraph results below come from a different git worktree ({}), \
         not where you're working ({}) — they may reflect another branch, \
         and symbols changed only here are missing. Run \"rustcodegraph init -i\" here for a \
         worktree-local index.",
        m.index_root, m.worktree_root
    )
}

fn realpath(path: impl AsRef<Path>) -> String {
    // 先 absolutize 再 canonicalize；路径不存在时也能返回稳定的绝对字符串。
    let resolved = absolutize(path.as_ref());
    fs::canonicalize(&resolved)
        .unwrap_or(resolved)
        .to_string_lossy()
        .into_owned()
}

fn absolutize(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        // current_dir 失败极少见；退回 "." 能保持函数无 panic。
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    }
}
