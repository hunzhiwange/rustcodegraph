//! CLI query 与 MCP node/explore 输出使用的源码文件辅助。
//!
//! 索引里存相对路径；这里负责把用户输入路径规范化、读取磁盘源码，并生成带行号的
//! 小片段，方便 agent 把返回内容当作已读上下文使用。

use std::fs;
use std::path::{Path, PathBuf};

use super::QueryMatch;

pub(crate) fn normalize_index_path(file_path: &str, project_root: &Path) -> String {
    // 接受 Windows 分隔符、项目绝对路径和相对路径，最终折叠成索引表里的 slash path。
    let mut normalized = file_path.trim().replace('\\', "/");
    if normalized.is_empty() {
        return String::new();
    }

    let root = normalize_slashes(&project_root.to_string_lossy());
    let root = root.trim_end_matches('/');
    if normalized == root {
        normalized.clear();
    } else if let Some(rest) = normalized.strip_prefix(&format!("{root}/")) {
        normalized = rest.to_owned();
    } else {
        let path = PathBuf::from(&normalized);
        if path.is_absolute()
            && let Ok(relative) = path.strip_prefix(project_root)
        {
            normalized = normalize_slashes(&relative.to_string_lossy());
        }
    }

    collapse_slash_path(&normalized)
}

fn collapse_slash_path(path: &str) -> String {
    // 对相对路径保留前导 ..，但绝对路径中的 .. 会向上折叠，匹配 Path::components 的常见语义。
    let absolute = path.starts_with('/');
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." if !absolute && parts.last().is_none_or(|last| *last == "..") => {
                parts.push(part);
            }
            ".." => {
                let _ = parts.pop();
            }
            _ => parts.push(part),
        }
    }
    let collapsed = parts.join("/");
    if absolute && !collapsed.is_empty() {
        format!("/{collapsed}")
    } else {
        collapsed
    }
}

pub(crate) fn is_test_file(path: &str) -> bool {
    // 这是 CLI affected 的粗粒度测试识别，不尝试读取 package/pytest/cargo 配置。
    let lower = path.to_ascii_lowercase();
    lower.contains("__tests__/")
        || lower.contains("/tests/")
        || lower.contains("/test/")
        || lower.contains(".test.")
        || lower.contains(".spec.")
        || lower.ends_with("_test.rs")
        || lower.ends_with("_test.go")
        || lower.ends_with("test.py")
}

pub(crate) fn read_numbered_file_range(
    project_root: &Path,
    file_path: &str,
    offset: usize,
    limit: usize,
) -> Result<String, String> {
    // offset 是 1-based，limit 至少为 1；返回空字符串代表范围超出文件末尾。
    let path = project_root.join(file_path);
    let source = fs::read_to_string(&path)
        .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
    let start = offset.max(1);
    let end = start.saturating_add(limit.max(1)).saturating_sub(1);
    Ok(numbered_lines(&source, start, end))
}

pub(crate) fn read_node_source(
    project_root: &Path,
    node: &QueryMatch,
    context: usize,
) -> Result<String, String> {
    // 给符号前面留两行上下文，帮助 agent 看到装饰器、注释或导出修饰符。
    let path = project_root.join(&node.file_path);
    let source = fs::read_to_string(&path)
        .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
    let start = (node.start_line as usize).saturating_sub(2).max(1);
    let end = node
        .start_line
        .saturating_add(context as u64)
        .max(node.start_line) as usize;
    Ok(numbered_lines(&source, start, end))
}

fn numbered_lines(source: &str, start: usize, end: usize) -> String {
    // 使用 tab 分隔行号和源码，避免源码自身缩进被 padding 改写。
    source
        .lines()
        .enumerate()
        .filter_map(|(idx, line)| {
            let line_no = idx + 1;
            (line_no >= start && line_no <= end).then(|| format!("{line_no}\t{line}"))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) fn format_matches(matches: &[QueryMatch]) -> String {
    // 文本格式保持一行一个命中，适合 MCP tool result 和终端管道继续处理。
    if matches.is_empty() {
        return "No results".to_owned();
    }
    matches
        .iter()
        .map(|node| {
            format!(
                "{}  {}  {}:{}",
                node.kind, node.name, node.file_path, node.start_line
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) fn normalize_slashes(path: &str) -> String {
    path.replace('\\', "/")
}
