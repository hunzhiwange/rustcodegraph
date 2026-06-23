use std::fs;
use std::path::Path;
use std::process::Command;

use crate::directory::is_code_graph_data_dir;

use super::constants::{
    DEFAULT_IGNORE_DIRS, DEFAULT_IGNORE_PATTERNS, EMBEDDED_REPO_SEARCH_DEPTH,
    EMBEDDED_REPO_SEARCH_ENTRIES,
};
use super::helpers::{normalize_slashes, pattern_matches};

/// 默认忽略器合并内置规则和当前目录的 `.gitignore`，供非 Git walk 路径使用。
#[derive(Debug, Clone)]
pub struct DefaultIgnore {
    patterns: Vec<String>,
}

impl DefaultIgnore {
    pub fn ignores(&self, relative_path: &str) -> bool {
        let normalized = normalize_slashes(relative_path);
        let parts = normalized.split('/').collect::<Vec<_>>();
        if parts.iter().any(|part| DEFAULT_IGNORE_DIRS.contains(part)) {
            return true;
        }
        self.patterns
            .iter()
            .any(|pattern| pattern_matches(pattern, &normalized))
    }
}

#[derive(Debug, Clone)]
pub struct ScopeIgnore {
    root_matcher: DefaultIgnore,
    embedded: Vec<(String, DefaultIgnore)>,
    nested: Vec<(String, DefaultIgnore)>,
    defaults: DefaultIgnore,
}

impl ScopeIgnore {
    pub fn new(
        root_matcher: DefaultIgnore,
        embedded: Vec<(String, DefaultIgnore)>,
        nested: Vec<(String, DefaultIgnore)>,
    ) -> Self {
        let defaults = defaults_only_ignore(Path::new("."));
        let mut embedded = embedded;
        // 更深的根先匹配，避免 `packages/a` 被更短的 `packages` 抢先处理。
        embedded.sort_by_key(|(root, _)| std::cmp::Reverse(root.len()));
        let mut nested = nested;
        nested.sort_by_key(|(root, _)| std::cmp::Reverse(root.len()));
        Self {
            root_matcher,
            embedded,
            nested,
            defaults,
        }
    }

    pub fn ignores(&self, rel: &str) -> bool {
        let rel = normalize_slashes(rel);
        if is_code_graph_data_path(&rel) {
            return true;
        }
        // 普通嵌套 .gitignore 只影响它所在的子树，不会重置外层默认忽略语义。
        for (root, matcher) in &self.nested {
            if rel == *root || rel.starts_with(&format!("{root}/")) {
                let inner = rel.strip_prefix(root).unwrap_or("").trim_start_matches('/');
                if matcher.ignores(inner) {
                    return true;
                }
            }
        }
        // 嵌入式 Git 仓库拥有独立 ignore 规则；同时仍保留外层的内置默认规则，
        // 防止 node_modules 等大目录在嵌入式仓库里被重新扫入。
        for (root, matcher) in &self.embedded {
            if rel == *root || rel.starts_with(&format!("{root}/")) {
                let inner = rel.strip_prefix(root).unwrap_or("").trim_start_matches('/');
                return matcher.ignores(inner) || self.defaults.ignores(&rel);
            }
        }
        // walk 到嵌入式仓库的父目录时不能提前忽略，否则后续无法进入真正的仓库根。
        if rel.ends_with('/') && self.embedded.iter().any(|(root, _)| root.starts_with(&rel)) {
            return false;
        }
        self.root_matcher.ignores(&rel)
    }
}

pub(super) fn is_code_graph_data_path(rel: &str) -> bool {
    let top = rel.trim_end_matches('/').split('/').next().unwrap_or(rel);
    is_code_graph_data_dir(top)
}

pub fn build_default_ignore(root_dir: impl AsRef<Path>) -> DefaultIgnore {
    let root_dir = root_dir.as_ref().to_path_buf();
    let mut patterns = DEFAULT_IGNORE_PATTERNS
        .iter()
        .map(|pattern| (*pattern).to_owned())
        .collect::<Vec<_>>();
    let gitignore = root_dir.join(".gitignore");
    if let Ok(content) = fs::read_to_string(gitignore) {
        patterns.extend(read_gitignore_patterns(&content));
    }
    DefaultIgnore { patterns }
}

fn defaults_only_ignore(root_dir: impl AsRef<Path>) -> DefaultIgnore {
    let _ = root_dir;
    DefaultIgnore {
        patterns: DEFAULT_IGNORE_PATTERNS
            .iter()
            .map(|pattern| (*pattern).to_owned())
            .collect(),
    }
}

fn read_gitignore_patterns(content: &str) -> Vec<String> {
    content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_owned)
        .collect()
}

fn list_ignored_dirs(repo_dir: &Path) -> Vec<String> {
    // `git status --ignored` 可以发现被外层 .gitignore 隐藏的嵌入式仓库，
    // 这类目录不会出现在 ls-files 里，但用户仍可能希望单独索引其源码。
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_dir)
        .args(["status", "--ignored", "--porcelain=v1", "-z"])
        .output();
    let Ok(output) = output else {
        return Vec::new();
    };
    String::from_utf8_lossy(&output.stdout)
        .split('\0')
        .filter_map(|entry| entry.strip_prefix("!! "))
        .filter(|entry| entry.ends_with('/'))
        .map(|entry| entry.trim_end_matches('/').to_owned())
        .collect()
}

fn classify_git_dir(abs_dir: &Path) -> GitDirKind {
    let git = abs_dir.join(".git");
    if git.is_dir() {
        GitDirKind::Embedded
    } else if git.is_file() {
        GitDirKind::Worktree
    } else {
        GitDirKind::None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GitDirKind {
    Embedded,
    Worktree,
    None,
}

fn find_nested_git_repos(abs_dir: &Path, rel_prefix: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut queue = vec![(abs_dir.to_path_buf(), rel_prefix.to_owned(), 0usize)];
    let mut visited = 0usize;

    while let Some((dir, prefix, depth)) = queue.pop() {
        visited += 1;
        // 探测只做浅层启发式，避免在大型 vendor/cache 树里为寻找 .git 付出过高成本。
        if visited > EMBEDDED_REPO_SEARCH_ENTRIES || depth > EMBEDDED_REPO_SEARCH_DEPTH {
            break;
        }
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if !file_type.is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if DEFAULT_IGNORE_DIRS.contains(&name.as_str()) {
                continue;
            }
            let rel = if prefix.is_empty() {
                name
            } else {
                format!("{prefix}/{name}")
            };
            match classify_git_dir(&path) {
                GitDirKind::Embedded => out.push(rel),
                GitDirKind::Worktree | GitDirKind::None => {
                    queue.push((path, rel, depth + 1));
                }
            }
        }
    }

    out
}

pub fn build_scope_ignore(
    root_dir: impl AsRef<Path>,
    embedded_roots: Option<Vec<String>>,
) -> ScopeIgnore {
    let root_dir = root_dir.as_ref();
    let roots = embedded_roots.unwrap_or_else(|| discover_embedded_repo_roots(root_dir));
    // 一次构建出“根规则 + 普通嵌套规则 + 嵌入式仓库规则”，扫描和 watcher 可共享同一语义。
    let nested = discover_nested_gitignore_roots(root_dir)
        .into_iter()
        .map(|root| {
            let matcher = build_default_ignore(root_dir.join(&root));
            (root, matcher)
        })
        .collect();
    let embedded = roots
        .into_iter()
        .map(|root| {
            let matcher = build_default_ignore(root_dir.join(&root));
            (root, matcher)
        })
        .collect();
    ScopeIgnore::new(build_default_ignore(root_dir), embedded, nested)
}

pub fn discover_embedded_repo_roots(root_dir: impl AsRef<Path>) -> Vec<String> {
    let root_dir = root_dir.as_ref();
    let mut roots = find_nested_git_repos(root_dir, "");
    roots.extend(find_ignored_embedded_repos(root_dir));
    roots.sort();
    roots.dedup();
    roots
}

fn find_ignored_embedded_repos(repo_dir: &Path) -> Vec<String> {
    list_ignored_dirs(repo_dir)
        .into_iter()
        .filter(|rel| repo_dir.join(rel).join(".git").is_dir())
        .collect()
}

fn discover_nested_gitignore_roots(root_dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut queue = vec![(root_dir.to_path_buf(), String::new(), 0usize)];
    let mut visited = 0usize;

    while let Some((dir, prefix, depth)) = queue.pop() {
        visited += 1;
        // 和嵌入式仓库探测共用预算，保证初始化耗时随目录规模有上界。
        if visited > EMBEDDED_REPO_SEARCH_ENTRIES || depth > EMBEDDED_REPO_SEARCH_DEPTH {
            break;
        }
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if !file_type.is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if DEFAULT_IGNORE_DIRS.contains(&name.as_str()) || is_code_graph_data_dir(&name) {
                continue;
            }
            let rel = if prefix.is_empty() {
                name
            } else {
                format!("{prefix}/{name}")
            };
            if path.join(".gitignore").is_file() {
                out.push(rel.clone());
            }
            queue.push((path, rel, depth + 1));
        }
    }

    out.sort();
    out.dedup();
    out
}
