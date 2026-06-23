use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::extraction::grammars::is_source_file;

use super::constants::MAX_FILE_SIZE;
use super::helpers::normalize_slashes;
use super::ignore::{
    ScopeIgnore, build_scope_ignore, discover_embedded_repo_roots, is_code_graph_data_path,
};
use super::results::GitChanges;

// Git 仓库优先用 git 自己的可见文件列表：它天然理解 .gitignore、
// submodule 和 untracked 文件，比手写 walk 更接近用户实际编辑的代码集合。
fn collect_git_files(
    repo_dir: &Path,
    prefix: &str,
    files: &mut HashSet<String>,
    embedded_roots: Option<&HashSet<String>>,
) {
    let tracked = Command::new("git")
        .arg("-C")
        .arg(repo_dir)
        .args(["ls-files", "-z", "-c", "--recurse-submodules"])
        .output();
    if let Ok(output) = tracked {
        for rel in String::from_utf8_lossy(&output.stdout).split('\0') {
            push_git_file(prefix, rel, files, embedded_roots);
        }
    }

    let untracked = Command::new("git")
        .arg("-C")
        .arg(repo_dir)
        .args(["ls-files", "-z", "-o", "--exclude-standard"])
        .output();
    if let Ok(output) = untracked {
        for rel in String::from_utf8_lossy(&output.stdout).split('\0') {
            push_git_file(prefix, rel, files, embedded_roots);
        }
    }
}

fn push_git_file(
    prefix: &str,
    rel: &str,
    files: &mut HashSet<String>,
    embedded_roots: Option<&HashSet<String>>,
) {
    if rel.is_empty() {
        return;
    }
    let joined = if prefix.is_empty() {
        normalize_slashes(rel)
    } else {
        normalize_slashes(&format!("{prefix}/{rel}"))
    };
    if is_code_graph_data_path(&joined) {
        return;
    }
    // 根仓库的 ls-files 会把嵌套仓库当普通目录列出；这里先剔除，
    // 后面再按嵌套仓库自己的 Git 视角收集，避免 ignore 语义串台。
    if embedded_roots
        .map(|roots| {
            roots
                .iter()
                .any(|root| joined.starts_with(&format!("{root}/")))
        })
        .unwrap_or(false)
    {
        return;
    }
    if is_source_file(&joined) {
        files.insert(joined);
    }
}

fn get_git_visible_files(root_dir: impl AsRef<Path>) -> Option<HashSet<String>> {
    let root_dir = root_dir.as_ref();
    if !root_dir.join(".git").exists() || !is_git_work_tree(root_dir) {
        return None;
    }
    let embedded_roots = discover_embedded_repo_roots(root_dir)
        .into_iter()
        .collect::<HashSet<_>>();
    let mut files = HashSet::new();
    collect_git_files(root_dir, "", &mut files, Some(&embedded_roots));
    // 嵌入式仓库有自己的 tracked/untracked 集合，必须用它自己的 cwd 查询，
    // 再加回外层相对路径前缀。
    for root in &embedded_roots {
        collect_git_files(&root_dir.join(root), root, &mut files, None);
    }
    Some(files)
}

fn is_git_work_tree(root_dir: &Path) -> bool {
    Command::new("git")
        .arg("-C")
        .arg(root_dir)
        .args(["rev-parse", "--is-inside-work-tree"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim() == "true")
        .unwrap_or(false)
}

pub(super) fn get_git_changed_files(root_dir: impl AsRef<Path>) -> Option<GitChanges> {
    let root_dir = root_dir.as_ref();
    if !root_dir.join(".git").exists() {
        return None;
    }
    let output = Command::new("git")
        .arg("-C")
        .arg(root_dir)
        .args(["status", "--porcelain=v1", "-z"])
        .output()
        .ok()?;
    let mut changes = GitChanges::default();
    // porcelain -z 输出紧凑且稳定；这里只需要粗略分成增/改/删，
    // 后续同步会重新解析增改文件并删除已移除文件的图数据。
    for entry in String::from_utf8_lossy(&output.stdout).split('\0') {
        if entry.len() < 4 {
            continue;
        }
        let status = &entry[..2];
        let rel = normalize_slashes(entry[3..].trim());
        if !is_source_file(&rel) {
            continue;
        }
        if status.contains('D') {
            changes.deleted.push(rel);
        } else if status.contains('?') || status.contains('A') {
            changes.added.push(rel);
        } else {
            changes.modified.push(rel);
        }
    }
    Some(changes)
}

pub fn scan_directory(
    root_dir: impl AsRef<Path>,
    mut on_progress: Option<&mut dyn FnMut(usize, String)>,
) -> Vec<String> {
    let root_dir = root_dir.as_ref();
    // Git 快路径失败时才回退到文件系统遍历；这让非 Git 临时目录和测试夹具仍可索引。
    if let Some(files) = get_git_visible_files(root_dir) {
        let mut files = files.into_iter().collect::<Vec<_>>();
        files.sort();
        for (idx, file) in files.iter().enumerate() {
            if let Some(callback) = on_progress.as_deref_mut() {
                callback(idx + 1, file.clone());
            }
        }
        return files;
    }
    scan_directory_walk(root_dir, on_progress)
}

pub async fn scan_directory_async(
    root_dir: impl AsRef<Path>,
    on_progress: Option<&mut dyn FnMut(usize, String)>,
) -> Vec<String> {
    scan_directory(root_dir, on_progress)
}

fn scan_directory_walk(
    root_dir: &Path,
    mut on_progress: Option<&mut dyn FnMut(usize, String)>,
) -> Vec<String> {
    let scope_ignore = build_scope_ignore(root_dir, None);
    let mut files = Vec::new();
    let mut visited_dirs = HashSet::new();
    walk_dir(
        root_dir,
        root_dir,
        &scope_ignore,
        &mut visited_dirs,
        &mut files,
        &mut on_progress,
    );
    files.sort();
    files
}

fn walk_dir(
    root_dir: &Path,
    dir: &Path,
    scope_ignore: &ScopeIgnore,
    visited_dirs: &mut HashSet<PathBuf>,
    files: &mut Vec<String>,
    on_progress: &mut Option<&mut dyn FnMut(usize, String)>,
) {
    let Ok(canonical) = fs::canonicalize(dir) else {
        return;
    };
    // follow symlink 时用 canonical 路径去重，防止循环链接导致无限递归。
    if !visited_dirs.insert(canonical) {
        return;
    }

    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(rel) = path.strip_prefix(root_dir) else {
            continue;
        };
        let rel = normalize_slashes(&rel.to_string_lossy());
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            if !scope_ignore.ignores(&rel) {
                walk_dir(
                    root_dir,
                    &path,
                    scope_ignore,
                    visited_dirs,
                    files,
                    on_progress,
                );
            }
        } else if file_type.is_file() && is_source_file(&rel) && !scope_ignore.ignores(&rel) {
            if let Ok(meta) = entry.metadata()
                && meta.len() > MAX_FILE_SIZE
            {
                continue;
            }
            files.push(rel.clone());
            if let Some(callback) = on_progress.as_deref_mut() {
                callback(files.len(), rel);
            }
        } else if file_type.is_symlink() {
            let Ok(metadata) = fs::metadata(&path) else {
                continue;
            };
            // 对符号链接只跟随到真实目录/文件，并沿用同一套 ignore 和大小限制。
            if metadata.is_dir() {
                if !scope_ignore.ignores(&rel) {
                    walk_dir(
                        root_dir,
                        &path,
                        scope_ignore,
                        visited_dirs,
                        files,
                        on_progress,
                    );
                }
            } else if metadata.is_file() && is_source_file(&rel) && !scope_ignore.ignores(&rel) {
                if metadata.len() > MAX_FILE_SIZE {
                    continue;
                }
                files.push(rel.clone());
                if let Some(callback) = on_progress.as_deref_mut() {
                    callback(files.len(), rel);
                }
            }
        }
    }
}
