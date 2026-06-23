//! Daemon socket and lockfile path helpers translated from `daemon-paths.ts`.
//!
//! socket / pidfile 路径必须只由 project root 决定，这样 proxy、daemon 和
//! registry 在不同进程中能算出同一位置。

use std::env;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::directory::get_code_graph_dir;

/// Soft upper bound for in-project POSIX socket paths.
pub const POSIX_SOCKET_PATH_LIMIT: usize = 100;

/// Structured contents of the daemon pid lockfile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DaemonLockInfo {
    pub pid: u32,
    pub version: String,
    pub socket_path: String,
    pub started_at: i64,
}

/// Short stable identifier for a project root, used in tmpdir / pipe names.
pub fn project_hash(project_root: impl AsRef<Path>) -> String {
    let resolved = project_root
        .as_ref()
        .canonicalize()
        .unwrap_or_else(|_| project_root.as_ref().to_path_buf());
    let mut hasher = Sha256::new();
    hasher.update(resolved.to_string_lossy().as_bytes());
    let digest = hasher.finalize();
    hex_prefix(&digest, 16)
}

/// Compute the daemon socket / named-pipe path for `project_root`.
pub fn get_daemon_socket_path(project_root: impl AsRef<Path>) -> PathBuf {
    let project_root = project_root.as_ref();
    if cfg!(windows) {
        // Windows named pipe 不受 POSIX 路径长度限制，直接用项目 hash。
        return PathBuf::from(format!(
            r"\\.\pipe\rustcodegraph-{}",
            project_hash(project_root)
        ));
    }

    let in_project = get_code_graph_dir(project_root).join("daemon.sock");
    if in_project.to_string_lossy().len() <= POSIX_SOCKET_PATH_LIMIT {
        return in_project;
    }

    // macOS/Linux 的 Unix socket 路径长度很短；深层 repo 回退到 tmpdir，
    // 但仍用 project hash 保持稳定。
    env::temp_dir().join(format!("rustcodegraph-{}.sock", project_hash(project_root)))
}

/// Absolute path to the daemon pid lockfile for `project_root`.
pub fn get_daemon_pid_path(project_root: impl AsRef<Path>) -> PathBuf {
    get_code_graph_dir(project_root).join("daemon.pid")
}

/// Serialize a daemon lock for writing to the pidfile.
pub fn encode_lock_info(info: &DaemonLockInfo) -> String {
    match serde_json::to_string_pretty(info) {
        Ok(mut json) => {
            json.push('\n');
            json
        }
        Err(_) => String::new(),
    }
}

/// Parse a pidfile body, including the legacy plain-pid format.
pub fn decode_lock_info(raw: &str) -> Option<DaemonLockInfo> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Ok(parsed) = serde_json::from_str::<DaemonLockInfo>(trimmed)
        && parsed.pid > 0
        && !parsed.version.is_empty()
        && !parsed.socket_path.is_empty()
    {
        return Some(parsed);
    }

    trimmed
        .parse::<u32>()
        .ok()
        .filter(|pid| *pid > 0)
        .map(|pid| DaemonLockInfo {
            pid,
            version: "unknown".to_string(),
            socket_path: String::new(),
            started_at: 0,
        })
}

fn hex_prefix(bytes: &[u8], chars: usize) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(chars);
    for byte in bytes {
        if out.len() >= chars {
            break;
        }
        out.push(HEX[(byte >> 4) as usize] as char);
        if out.len() >= chars {
            break;
        }
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}
