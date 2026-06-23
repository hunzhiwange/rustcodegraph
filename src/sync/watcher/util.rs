//! watcher 内部小工具。
//!
//! 这些 helper 保持无状态，集中处理路径归一化、测试环境识别和日志 context 构造。

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::Value;

use crate::errors::ErrorContext;
use crate::utils::normalize_path;

pub(super) fn watch_registry_key(path: &Path) -> PathBuf {
    // registry key 尽量 canonicalize，避免符号链接或相对路径让同一项目注册两份状态。
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

pub(super) fn is_test_runtime() -> bool {
    // Rust 测试沿用 JS 侧环境变量约定，便于从 Vitest/Node harness 驱动同一套 seam。
    std::env::var_os("VITEST").is_some()
        || std::env::var("NODE_ENV")
            .map(|value| value == "test")
            .unwrap_or(false)
}

pub(super) fn now_ms() -> i64 {
    // watcher 的时间戳只用于排序和展示年龄；系统时间异常时退回 0，避免 panic。
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_millis() as i64
}

pub(super) fn relative_posix(root: &Path, path: &Path) -> String {
    // 对外 pending/stale 提示统一使用 POSIX 分隔符，即使运行在 Windows。
    path.strip_prefix(root)
        .map(path_to_posix)
        .unwrap_or_else(|_| path_to_posix(path))
}

fn path_to_posix(path: &Path) -> String {
    normalize_path(&path.to_string_lossy())
}

pub(super) fn path_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

pub(super) fn context(entries: impl IntoIterator<Item = (&'static str, Value)>) -> ErrorContext {
    entries
        .into_iter()
        .map(|(key, value)| (key.to_owned(), value))
        .collect()
}
