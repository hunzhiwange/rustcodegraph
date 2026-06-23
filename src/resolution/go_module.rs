//! Go module path detection.
//!
//! Go 的跨包解析需要知道当前 module path；这里只读取 `go.mod` 的 module 行，
//! 不尝试解释 replace/workspace，保持解析入口轻量且可缓存。

use std::fs;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoModule {
    pub module_path: String,
    pub root_dir: String,
}

pub fn load_go_module(project_root: impl AsRef<Path>) -> Option<GoModule> {
    let project_root = project_root.as_ref();
    let content = fs::read_to_string(project_root.join("go.mod")).ok()?;
    // module 行后可能跟注释；先剥掉 `//`，避免把注释文字带进 module path。
    let stripped = content
        .lines()
        .map(|line| line.split_once("//").map(|(head, _)| head).unwrap_or(line))
        .collect::<Vec<_>>()
        .join("\n");

    for line in stripped.lines() {
        let trimmed = line.trim();
        let Some(rest) = trimmed.strip_prefix("module") else {
            continue;
        };
        let mut module_path = rest.trim().to_string();
        if module_path.starts_with('"') || module_path.starts_with('\'') {
            module_path.remove(0);
        }
        if module_path.ends_with('"') || module_path.ends_with('\'') {
            module_path.pop();
        }
        if module_path.is_empty() {
            return None;
        }
        return Some(GoModule {
            module_path,
            root_dir: project_root.to_string_lossy().into_owned(),
        });
    }

    None
}
