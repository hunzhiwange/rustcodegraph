//! Global daemon registry and stop/list control translated from
//! `daemon-registry.ts`.
//!
//! registry 是 daemon pidfile 之外的发现索引：pidfile 按项目存放，registry
//! 让 `rustcodegraph daemon` 这类全局管理命令能枚举所有项目。

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::daemon_paths::{decode_lock_info, get_daemon_pid_path, get_daemon_socket_path};

pub use super::daemon::is_process_alive;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DaemonRecord {
    pub root: String,
    pub pid: u32,
    pub version: String,
    pub socket_path: String,
    pub started_at: i64,
}

pub fn get_registry_dir() -> PathBuf {
    home_dir()
        .unwrap_or_else(env::temp_dir)
        .join(".rustcodegraph")
        .join("daemons")
}

pub fn register_daemon(rec: &DaemonRecord) {
    let _ = fs::create_dir_all(get_registry_dir());
    if let Ok(mut json) = serde_json::to_string_pretty(rec) {
        json.push('\n');
        let _ = fs::write(record_path(&rec.root), json);
    }
}

pub fn deregister_daemon(root: impl AsRef<Path>) {
    let _ = fs::remove_file(record_path(root));
}

pub fn list_daemons(prune: bool) -> Vec<DaemonRecord> {
    // 读取 registry 时顺便验证 pid 是否仍存活；prune=true 的调用会清理
    // 崩溃后遗留的记录，避免交互列表越来越脏。
    let Ok(entries) = fs::read_dir(get_registry_dir()) else {
        return Vec::new();
    };

    let mut live = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let rec = fs::read_to_string(&path)
            .ok()
            .and_then(|raw| serde_json::from_str::<DaemonRecord>(&raw).ok());
        if rec
            .as_ref()
            .is_some_and(|rec| rec.pid > 0 && !rec.root.is_empty() && is_process_alive(rec.pid))
        {
            live.push(rec.unwrap());
        } else if prune {
            let _ = fs::remove_file(path);
        }
    }

    live.sort_by_key(|entry| std::cmp::Reverse(entry.started_at));
    live
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StopOutcome {
    Term,
    Kill,
    NotRunning,
    NoDaemon,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StopResult {
    pub root: String,
    pub pid: Option<u32>,
    pub outcome: StopOutcome,
}

pub fn stop_daemon_at(root: impl AsRef<Path>) -> StopResult {
    // 优先信任项目 pidfile；缺失时再查全局 registry，兼容旧版本或手动清理
    // `.rustcodegraph/daemon.pid` 后仍能停止 daemon。
    let root = root.as_ref();
    let root_string = root.to_string_lossy().to_string();
    let mut pid = fs::read_to_string(get_daemon_pid_path(root))
        .ok()
        .and_then(|raw| decode_lock_info(&raw))
        .map(|info| info.pid);

    if pid.is_none() {
        pid = list_daemons(false)
            .into_iter()
            .find(|rec| same_path(&rec.root, root))
            .map(|rec| rec.pid);
    }

    let Some(pid) = pid else {
        cleanup_daemon_artifacts(root);
        return StopResult {
            root: root_string,
            pid: None,
            outcome: StopOutcome::NoDaemon,
        };
    };

    if !is_process_alive(pid) {
        cleanup_daemon_artifacts(root);
        return StopResult {
            root: root_string,
            pid: Some(pid),
            outcome: StopOutcome::NotRunning,
        };
    }

    send_signal(pid, "TERM");
    let mut outcome = StopOutcome::Term;
    if !wait_for_death(pid, Duration::from_millis(3_000)) {
        send_signal(pid, "KILL");
        let _ = wait_for_death(pid, Duration::from_millis(2_000));
        outcome = StopOutcome::Kill;
    }
    cleanup_daemon_artifacts(root);
    StopResult {
        root: root_string,
        pid: Some(pid),
        outcome,
    }
}

pub fn stop_all_daemons() -> Vec<StopResult> {
    list_daemons(true)
        .into_iter()
        .map(|rec| stop_daemon_at(rec.root))
        .collect()
}

fn cleanup_daemon_artifacts(root: impl AsRef<Path>) {
    // Windows named pipe 不对应普通文件；POSIX socket 文件需要显式移除。
    let root = root.as_ref();
    let _ = fs::remove_file(get_daemon_pid_path(root));
    if !cfg!(windows) {
        let _ = fs::remove_file(get_daemon_socket_path(root));
    }
    deregister_daemon(root);
}

fn record_path(root: impl AsRef<Path>) -> PathBuf {
    let resolved = root
        .as_ref()
        .canonicalize()
        .unwrap_or_else(|_| root.as_ref().to_path_buf());
    let mut hasher = Sha256::new();
    hasher.update(resolved.to_string_lossy().as_bytes());
    let digest = hasher.finalize();
    let mut hash = String::with_capacity(16);
    for byte in digest.iter().take(8) {
        hash.push_str(&format!("{byte:02x}"));
    }
    get_registry_dir().join(format!("{hash}.json"))
}

fn same_path(a: &str, b: &Path) -> bool {
    let a = Path::new(a)
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(a));
    let b = b.canonicalize().unwrap_or_else(|_| b.to_path_buf());
    a == b
}

fn wait_for_death(pid: u32, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if !is_process_alive(pid) {
            return true;
        }
        thread::sleep(Duration::from_millis(100));
    }
    !is_process_alive(pid)
}

#[cfg(unix)]
fn send_signal(pid: u32, signal: &str) {
    unsafe extern "C" {
        fn kill(pid: i32, sig: i32) -> i32;
    }
    let sig = if signal == "KILL" { 9 } else { 15 };
    unsafe {
        let _ = kill(pid as i32, sig);
    }
}

#[cfg(windows)]
fn send_signal(pid: u32, signal: &str) {
    let force = if signal == "KILL" { "/F" } else { "" };
    let _ = std::process::Command::new("cmd")
        .args(["/C", "taskkill", "/PID", &pid.to_string(), force])
        .output();
}

#[cfg(all(not(unix), not(windows)))]
fn send_signal(_pid: u32, _signal: &str) {}

fn home_dir() -> Option<PathBuf> {
    env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .or_else(|| {
            let drive = env::var_os("HOMEDRIVE")?;
            let path = env::var_os("HOMEPATH")?;
            let mut joined = PathBuf::from(drive);
            joined.push(path);
            Some(joined.into_os_string())
        })
        .map(PathBuf::from)
}
