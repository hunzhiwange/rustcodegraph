//! Watch policy translated from `watch-policy.ts`.
//!
//! This centralizes the decision about whether live file watching should run.
//!
//! 中文维护提示：这里返回“为什么禁用 watcher”，上层再决定是否安装 git hook
//! 或提示用户；策略顺序要保持显式环境变量优先。

use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::Path;
use std::sync::{LazyLock, Mutex};

use crate::utils::normalize_path;

static WSL_CACHE: LazyLock<Mutex<(bool, bool)>> = LazyLock::new(|| Mutex::new((false, false)));

/// Inputs that tests can override so policy decisions are deterministic.
#[derive(Debug, Clone, Default)]
pub struct WatchProbe {
    /// Defaults to the process environment.
    pub env: Option<HashMap<String, String>>,
    /// Defaults to `detect_wsl()`.
    pub is_wsl: Option<bool>,
}

/// Detect whether the current process is running under WSL.
///
/// The result is cached after the first call, matching the TypeScript module.
pub fn detect_wsl() -> bool {
    // WSL 检测会触碰环境和 `/proc/version`，缓存后避免每次 init/watch 都重复 IO。
    if let Ok(cache) = WSL_CACHE.lock()
        && cache.0
    {
        return cache.1;
    }

    let value = if !cfg!(target_os = "linux") {
        false
    } else if env::var_os("WSL_DISTRO_NAME").is_some() || env::var_os("WSL_INTEROP").is_some() {
        true
    } else {
        fs::read_to_string("/proc/version")
            .map(|version| {
                let lower = version.to_ascii_lowercase();
                lower.contains("microsoft") || lower.contains("wsl")
            })
            .unwrap_or(false)
    };

    if let Ok(mut cache) = WSL_CACHE.lock() {
        *cache = (true, value);
    }
    value
}

/// Decide whether the file watcher should be disabled for a project, and why.
///
/// Precedence:
/// 1. `RUSTCODEGRAPH_NO_WATCH=1`
/// 2. `RUSTCODEGRAPH_FORCE_WATCH=1`
/// 3. WSL2 `/mnt/<drive>` paths
pub fn watch_disabled_reason(
    project_root: impl AsRef<Path>,
    probe: Option<&WatchProbe>,
) -> Option<String> {
    // 手动禁用永远优先；force watch 用于用户知道自己环境可承受 watcher 的场景。
    if let Some(key) = env_flag_is_one(probe, &["RUSTCODEGRAPH_NO_WATCH"]) {
        return Some(format!("{key}=1 is set"));
    }
    if env_flag_is_one(probe, &["RUSTCODEGRAPH_FORCE_WATCH"]).is_some() {
        return None;
    }

    let is_wsl = probe.and_then(|p| p.is_wsl).unwrap_or_else(detect_wsl);
    if is_wsl && is_windows_drive_mount(project_root.as_ref()) {
        return Some(
            "project is on a WSL2 /mnt/ drive, where recursive fs.watch is too slow to be reliable"
                .to_owned(),
        );
    }

    None
}

/// Test-only: reset the cached WSL detection.
pub fn __reset_wsl_cache_for_tests() {
    if let Ok(mut cache) = WSL_CACHE.lock() {
        *cache = (false, false);
    }
}

fn env_flag_is_one(probe: Option<&WatchProbe>, keys: &[&'static str]) -> Option<&'static str> {
    keys.iter()
        .copied()
        .find(|key| env_value(probe, key).as_deref() == Some("1"))
}

fn env_value(probe: Option<&WatchProbe>, key: &str) -> Option<String> {
    if let Some(env) = probe.and_then(|p| p.env.as_ref()) {
        return env.get(key).cloned();
    }
    env::var(key).ok()
}

fn is_windows_drive_mount(project_root: &Path) -> bool {
    // 只匹配 WSL 的 `/mnt/<drive>` 根或子路径；Linux 普通 `/mnt/project` 不应被误伤。
    let normalized = normalize_path(&project_root.to_string_lossy());
    let Some(rest) = normalized.strip_prefix("/mnt/") else {
        return false;
    };
    let mut chars = rest.chars();
    let Some(drive) = chars.next() else {
        return false;
    };
    drive.is_ascii_alphabetic() && matches!(chars.next(), None | Some('/'))
}
