//! Package-version resolution translated from `version.ts`.
//!
//! MCP proxy/daemon 握手需要当前包版本。发布后的二进制、源码运行和测试运行的
//! `package.json` 相对位置不同，所以这里按多个候选路径兜底查找。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use serde_json::Value;

pub const UNKNOWN_VERSION: &str = "0.0.0-unknown";

/// Resolved package version, computed once at module load.
pub static CODE_GRAPH_PACKAGE_VERSION: LazyLock<String> = LazyLock::new(read_package_version);

pub fn code_graph_package_version() -> &'static str {
    CODE_GRAPH_PACKAGE_VERSION.as_str()
}

fn read_package_version() -> String {
    // 版本读取失败不能阻塞 MCP 启动；unknown 会让兼容性检查走保守分支。
    for candidate in package_json_candidates() {
        let Ok(raw) = fs::read_to_string(&candidate) else {
            continue;
        };
        let Ok(parsed) = serde_json::from_str::<Value>(&raw) else {
            continue;
        };
        if let Some(version) = parsed.get("version").and_then(Value::as_str)
            && !version.is_empty()
        {
            return version.to_string();
        }
    }
    UNKNOWN_VERSION.to_string()
}

fn package_json_candidates() -> Vec<PathBuf> {
    // 覆盖 cargo test、仓库根目录 cargo run、以及 npm/cargo-dist 打包后从二进制
    // 目录启动的几种布局。
    let mut out = Vec::new();
    if let Ok(dir) = std::env::var("CARGO_MANIFEST_DIR") {
        out.push(Path::new(&dir).join("package.json"));
    }
    if let Ok(cwd) = std::env::current_dir() {
        out.push(cwd.join("package.json"));
        out.push(cwd.join("..").join("package.json"));
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        out.push(dir.join("package.json"));
        out.push(dir.join("..").join("package.json"));
        out.push(dir.join("..").join("..").join("package.json"));
    }
    out
}
