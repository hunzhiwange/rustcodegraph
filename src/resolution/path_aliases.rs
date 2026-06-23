//! Project-level import-path alias loading.
//!
//! 中文维护提示：这里读取 tsconfig/jsconfig 的 `baseUrl` 和 `paths`，供 import
//! resolver 把 `@app/foo` 一类别名展开成项目内相对路径。

use std::fs;
use std::path::{Component, Path, PathBuf};

use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AliasPattern {
    pub prefix: String,
    pub suffix: String,
    pub has_wildcard: bool,
    pub replacements: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AliasMap {
    pub base_url: String,
    pub patterns: Vec<AliasPattern>,
}

fn strip_jsonc(src: &str) -> String {
    // tsconfig 通常是 JSONC：允许注释和尾逗号。这里做最小剥离后交给 serde_json，
    // 但保留字符串内容，避免路径里的 `//` 或 `/*` 被误删。
    let mut out = String::new();
    let mut chars = src.chars().peekable();
    let mut in_string = false;

    while let Some(ch) = chars.next() {
        if in_string {
            out.push(ch);
            if ch == '\\' {
                if let Some(next) = chars.next() {
                    out.push(next);
                }
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }

        if ch == '"' {
            in_string = true;
            out.push(ch);
            continue;
        }

        if ch == '/' && chars.peek().copied() == Some('/') {
            chars.next();
            for next in chars.by_ref() {
                if next == '\n' {
                    out.push('\n');
                    break;
                }
            }
            continue;
        }

        if ch == '/' && chars.peek().copied() == Some('*') {
            chars.next();
            let mut prev = '\0';
            for next in chars.by_ref() {
                if prev == '*' && next == '/' {
                    break;
                }
                prev = next;
            }
            continue;
        }

        out.push(ch);
    }

    strip_trailing_commas(&out)
}

fn strip_trailing_commas(src: &str) -> String {
    let mut out = String::new();
    let chars: Vec<char> = src.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == ',' {
            let mut j = i + 1;
            while j < chars.len() && chars[j].is_whitespace() {
                j += 1;
            }
            if j < chars.len() && (chars[j] == '}' || chars[j] == ']') {
                i += 1;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

fn read_tsconfig_like(path: &Path) -> Option<Value> {
    let raw = fs::read_to_string(path).ok()?;
    serde_json::from_str(&strip_jsonc(&raw)).ok()
}

fn split_wildcard(pattern: &str) -> (String, String, bool) {
    match pattern.find('*') {
        Some(star) => (
            pattern[..star].to_string(),
            pattern[star + 1..].to_string(),
            true,
        ),
        None => (pattern.to_string(), String::new(), false),
    }
}

pub fn load_project_aliases(project_root: impl AsRef<Path>) -> Option<AliasMap> {
    let project_root = project_root.as_ref();
    let raw = ["tsconfig.json", "jsconfig.json"].iter().find_map(|name| {
        let path = project_root.join(name);
        path.exists().then(|| read_tsconfig_like(&path)).flatten()
    })?;

    let compiler_options = raw.get("compilerOptions")?;
    let base_url_rel = compiler_options
        .get("baseUrl")
        .and_then(Value::as_str)
        .unwrap_or(".");
    let base_url = normalize_path(project_root.join(base_url_rel));
    let paths = compiler_options.get("paths")?.as_object()?;

    let mut patterns = Vec::new();
    for (pattern, targets) in paths {
        let Some(targets) = targets.as_array() else {
            continue;
        };
        let replacements = targets
            .iter()
            .filter_map(Value::as_str)
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        if replacements.is_empty() {
            continue;
        }
        let (prefix, suffix, has_wildcard) = split_wildcard(pattern);
        patterns.push(AliasPattern {
            prefix,
            suffix,
            has_wildcard,
            replacements,
        });
    }

    if patterns.is_empty() {
        return None;
    }

    // 长前缀优先，确保 `@app/*` 在更宽泛的 `@*` 之前命中；非通配模式在同前缀时
    // 更具体，也应排在前面。
    patterns.sort_by(|a, b| {
        b.prefix
            .len()
            .cmp(&a.prefix.len())
            .then_with(|| a.has_wildcard.cmp(&b.has_wildcard))
    });

    Some(AliasMap { base_url, patterns })
}

pub fn apply_aliases(import_path: &str, aliases: &AliasMap, project_root: &str) -> Vec<String> {
    let project_root = Path::new(project_root);
    for pattern in &aliases.patterns {
        if !import_path.starts_with(&pattern.prefix) {
            continue;
        }
        if !pattern.suffix.is_empty() && !import_path.ends_with(&pattern.suffix) {
            continue;
        }

        let captured = if pattern.has_wildcard {
            // wildcard 只替换第一处 `*`，与 TS paths 的单星号常见用法保持一致。
            let end = import_path.len().saturating_sub(pattern.suffix.len());
            import_path[pattern.prefix.len()..end].to_string()
        } else if import_path == pattern.prefix {
            String::new()
        } else {
            continue;
        };

        let mut out = Vec::new();
        for target in &pattern.replacements {
            let filled = if pattern.has_wildcard {
                target.replacen('*', &captured, 1)
            } else {
                target.clone()
            };
            let absolute = normalize_path_buf(Path::new(&aliases.base_url).join(filled));
            if let Some(relative) = strip_project_prefix(&absolute, project_root) {
                out.push(relative);
            }
        }
        return out;
    }
    Vec::new()
}

fn normalize_path(path: PathBuf) -> String {
    normalize_path_buf(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn normalize_path_buf(path: PathBuf) -> PathBuf {
    // 不触碰文件系统，只做词法归一化；这样别名目标即使还不存在也能产生候选路径。
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

fn strip_project_prefix(path: &Path, project_root: &Path) -> Option<String> {
    let project_root = normalize_path_buf(project_root.to_path_buf());
    let path = normalize_path_buf(path.to_path_buf());
    let rel = path.strip_prefix(&project_root).ok()?;
    Some(rel.to_string_lossy().replace('\\', "/"))
}
