//! Shared MCP daemon lifecycle translated from `daemon.ts`.
//!
//! This module preserves the daemon handshake, lockfile, idle-timeout, and
//! client-liveness data shapes. It intentionally does not bind sockets during
//! translation; `Daemon::start` reports that runtime wiring is deferred.
//!
//! daemon 是多 MCP 客户端共享一个 indexed project 的驻留进程。锁文件保证
//! 同一项目只启动一个 daemon，client hello 记录 host pid 以便宿主退出后清理连接。

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use super::daemon_paths::{
    DaemonLockInfo, decode_lock_info, encode_lock_info, get_daemon_pid_path, get_daemon_socket_path,
};
use super::version::code_graph_package_version;

pub const DEFAULT_IDLE_TIMEOUT_MS: u64 = 300_000;
pub const DEFAULT_MAX_IDLE_MS: u64 = 1_800_000;
pub const DEFAULT_CLIENT_SWEEP_MS: u64 = 30_000;
pub const CLIENT_HELLO_TIMEOUT_MS: u64 = 3_000;
pub const MAX_HELLO_LINE_BYTES: usize = 4096;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DaemonHello {
    #[serde(rename = "rustcodegraph")]
    pub rustcodegraph: String,
    pub pid: u32,
    #[serde(rename = "socketPath")]
    pub socket_path: String,
    pub protocol: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DaemonClientHello {
    #[serde(rename = "rustcodegraph_client")]
    pub rustcodegraph_client: u8,
    pub pid: u32,
    #[serde(rename = "hostPid")]
    pub host_pid: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonStartResult {
    pub socket_path: PathBuf,
    pub lock: DaemonLockInfo,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AcquireResult {
    Acquired {
        pid_path: PathBuf,
        info: DaemonLockInfo,
    },
    Taken {
        existing: Option<DaemonLockInfo>,
        pid_path: PathBuf,
    },
}

#[derive(Debug)]
pub struct Daemon {
    socket_path: PathBuf,
    pid_path: PathBuf,
    idle_timeout_ms: u64,
    max_idle_ms: u64,
    stopping: bool,
    clients: BTreeMap<DaemonClientHandle, DaemonClient>,
    stopped_clients: BTreeSet<DaemonClientHandle>,
    next_client_id: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonOptions {
    pub idle_timeout_ms: Option<u64>,
    pub max_idle_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonRuntimeDeferred;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DaemonClientHandle(u64);

#[derive(Debug, Clone, PartialEq, Eq)]
struct DaemonClient {
    peers: ClientPeerPids,
}

impl Daemon {
    pub fn new(project_root: impl Into<PathBuf>, opts: Option<DaemonOptions>) -> Self {
        let project_root = project_root.into();
        let opts = opts.unwrap_or(DaemonOptions {
            idle_timeout_ms: None,
            max_idle_ms: None,
        });
        Self {
            socket_path: get_daemon_socket_path(&project_root),
            pid_path: get_daemon_pid_path(&project_root),
            idle_timeout_ms: opts.idle_timeout_ms.unwrap_or_else(resolve_idle_timeout_ms),
            max_idle_ms: opts.max_idle_ms.unwrap_or_else(resolve_max_idle_ms),
            stopping: false,
            clients: BTreeMap::new(),
            stopped_clients: BTreeSet::new(),
            next_client_id: 0,
        }
    }

    /// Runtime socket binding is intentionally deferred for this translation.
    pub fn start(&mut self) -> Result<DaemonStartResult, DaemonRuntimeDeferred> {
        Err(DaemonRuntimeDeferred)
    }

    pub fn get_client_count(&self) -> usize {
        self.clients.len()
    }

    pub fn get_socket_path(&self) -> &Path {
        &self.socket_path
    }

    pub fn stop(&mut self, _reason: &str) {
        if self.stopping {
            return;
        }
        self.stopping = true;
        let handles = self.clients.keys().copied().collect::<Vec<_>>();
        for handle in handles {
            self.stop_client(handle);
        }
        self.cleanup_lockfile();
    }

    pub fn reap_dead_clients<F>(&mut self, is_alive: F) -> usize
    where
        F: Fn(u32) -> bool,
    {
        // 客户端进程或其宿主进程死亡都视为连接失效；否则 daemon 会因为
        // 半断开的 stdio/proxy 连接永远不进入 idle timeout。
        if self.clients.is_empty() {
            return 0;
        }

        let dead_clients = self
            .clients
            .iter()
            .filter_map(|(handle, client)| peer_is_dead(client.peers, &is_alive).then_some(*handle))
            .collect::<Vec<_>>();

        for handle in &dead_clients {
            if let Some(client) = self.clients.get(handle) {
                eprintln!(
                    "[RustCodeGraph daemon] Reaping client with dead peer (pid {}); clients={}.",
                    client
                        .peers
                        .pid
                        .map(|pid| pid.to_string())
                        .unwrap_or_else(|| "unknown".to_string()),
                    self.clients.len().saturating_sub(1)
                );
            }
            self.stop_client(*handle);
        }

        dead_clients.len()
    }

    pub fn idle_timeout_ms(&self) -> u64 {
        self.idle_timeout_ms
    }

    pub fn max_idle_ms(&self) -> u64 {
        self.max_idle_ms
    }

    #[doc(hidden)]
    pub fn add_client_for_liveness_test(&mut self, peers: ClientPeerPids) -> DaemonClientHandle {
        self.next_client_id += 1;
        let handle = DaemonClientHandle(self.next_client_id);
        self.clients.insert(handle, DaemonClient { peers });
        handle
    }

    #[doc(hidden)]
    pub fn has_client_for_liveness_test(&self, handle: DaemonClientHandle) -> bool {
        self.clients.contains_key(&handle)
    }

    #[doc(hidden)]
    pub fn client_peer_for_liveness_test(
        &self,
        handle: DaemonClientHandle,
    ) -> Option<ClientPeerPids> {
        self.clients.get(&handle).map(|client| client.peers)
    }

    #[doc(hidden)]
    pub fn client_was_stopped_for_liveness_test(&self, handle: DaemonClientHandle) -> bool {
        self.stopped_clients.contains(&handle)
    }

    fn stop_client(&mut self, handle: DaemonClientHandle) {
        if self.clients.remove(&handle).is_some() {
            self.stopped_clients.insert(handle);
        }
    }

    fn cleanup_lockfile(&self) {
        let Ok(raw) = fs::read_to_string(&self.pid_path) else {
            return;
        };
        let Some(info) = decode_lock_info(&raw) else {
            return;
        };
        if info.pid == std::process::id() {
            let _ = fs::remove_file(&self.pid_path);
        }
    }
}

/// Atomically create the daemon pidfile with its full record already in place.
pub fn try_acquire_daemon_lock(project_root: impl AsRef<Path>) -> AcquireResult {
    // 使用 hard_link 做原子占锁：tmp 文件先写完整 JSON，只有 link 成功的一方
    // 成为持有者，避免并发启动时读到半写 pidfile。
    let pid_path = get_daemon_pid_path(project_root.as_ref());
    if let Some(parent) = pid_path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    let info = DaemonLockInfo {
        pid: std::process::id(),
        version: code_graph_package_version().to_string(),
        socket_path: get_daemon_socket_path(project_root.as_ref())
            .to_string_lossy()
            .to_string(),
        started_at: now_ms(),
    };

    let tmp = pid_path.with_extension(format!("{}.tmp", std::process::id()));
    let mut acquired = false;
    let write_result = fs::write(&tmp, encode_lock_info(&info));
    if write_result.is_ok() {
        match fs::hard_link(&tmp, &pid_path) {
            Ok(()) => acquired = true,
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {}
            Err(_) => {}
        }
    }
    let _ = fs::remove_file(&tmp);

    if acquired {
        return AcquireResult::Acquired { pid_path, info };
    }

    let existing = fs::read_to_string(&pid_path)
        .ok()
        .and_then(|raw| decode_lock_info(&raw));
    AcquireResult::Taken { existing, pid_path }
}

/// Remove a stale pidfile only if it still names the dead process the caller saw.
pub fn clear_stale_daemon_lock(pid_path: impl AsRef<Path>, expected_dead_pid: Option<u32>) -> bool {
    // expected_dead_pid 防止 ABA：调用者判断 A 已死后，另一个 daemon B 可能
    // 已经拿到锁；此时不能删除 B 的新 pidfile。
    let pid_path = pid_path.as_ref();
    match fs::read_to_string(pid_path) {
        Ok(raw) => {
            if let Some(info) = decode_lock_info(&raw) {
                if expected_dead_pid.is_some_and(|pid| pid != info.pid) {
                    return false;
                }
                if info.pid > 0 && is_process_alive(info.pid) {
                    return false;
                }
            }
            fs::remove_file(pid_path).is_ok()
        }
        Err(err) if err.kind() == io::ErrorKind::NotFound => true,
        Err(_) => false,
    }
}

pub fn parse_client_hello_line(line: &str) -> Option<ClientPeerPids> {
    let parsed: DaemonClientHello = serde_json::from_str(line).ok()?;
    if parsed.rustcodegraph_client != 1 || parsed.pid == 0 {
        return None;
    }
    Some(ClientPeerPids {
        pid: Some(parsed.pid),
        host_pid: parsed.host_pid,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientPeerPids {
    pub pid: Option<u32>,
    pub host_pid: Option<u32>,
}

pub fn peer_is_dead<F>(peers: ClientPeerPids, is_alive: F) -> bool
where
    F: Fn(u32) -> bool,
{
    // host_pid 是 Cursor/Codex 等宿主进程；proxy 子进程还活着但宿主没了时，
    // 也应该释放 daemon client。
    let Some(pid) = peers.pid else {
        return false;
    };
    if !is_alive(pid) {
        return true;
    }
    peers.host_pid.is_some_and(|host_pid| !is_alive(host_pid))
}

pub fn daemon_hello(socket_path: impl AsRef<Path>) -> DaemonHello {
    DaemonHello {
        rustcodegraph: code_graph_package_version().to_string(),
        pid: std::process::id(),
        socket_path: socket_path.as_ref().to_string_lossy().to_string(),
        protocol: 1,
    }
}

pub fn resolve_idle_timeout_ms() -> u64 {
    parse_nonnegative_env_aliases(
        &["RUSTCODEGRAPH_DAEMON_IDLE_TIMEOUT_MS"],
        DEFAULT_IDLE_TIMEOUT_MS,
    )
}

pub fn resolve_max_idle_ms() -> u64 {
    parse_nonnegative_env_aliases(&["RUSTCODEGRAPH_DAEMON_MAX_IDLE_MS"], DEFAULT_MAX_IDLE_MS)
}

pub fn resolve_client_sweep_ms() -> u64 {
    parse_nonnegative_env_aliases(
        &["RUSTCODEGRAPH_DAEMON_CLIENT_SWEEP_MS"],
        DEFAULT_CLIENT_SWEEP_MS,
    )
}

fn parse_nonnegative_env_aliases(names: &[&str], fallback: u64) -> u64 {
    names
        .iter()
        .find_map(|name| parse_nonnegative_env(name, fallback))
        .unwrap_or(fallback)
}

fn parse_nonnegative_env(name: &str, fallback: u64) -> Option<u64> {
    let Ok(raw) = std::env::var(name) else {
        return None;
    };
    if raw.is_empty() {
        return Some(fallback);
    }
    Some(raw.parse::<u64>().unwrap_or(fallback))
}

pub fn is_process_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    platform_process_alive(pid)
}

#[cfg(unix)]
fn platform_process_alive(pid: u32) -> bool {
    // `kill(pid, 0)` 会把 zombie 也算作存在；再用 `ps` 过滤 Z 状态，避免
    // Linux 测试和 daemon 清理把已退出进程误判为存活。
    if pid > i32::MAX as u32 {
        return false;
    }

    unsafe extern "C" {
        fn kill(pid: i32, sig: i32) -> i32;
    }
    if unsafe { kill(pid as i32, 0) != 0 } {
        return false;
    }

    let Ok(output) = std::process::Command::new("ps")
        .args(["-o", "stat=", "-p", &pid.to_string()])
        .output()
    else {
        return true;
    };
    let stat = String::from_utf8_lossy(&output.stdout);
    !stat.trim_start().starts_with('Z')
}

#[cfg(windows)]
fn platform_process_alive(pid: u32) -> bool {
    use std::ffi::c_void;

    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    const STILL_ACTIVE: u32 = 259;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn OpenProcess(dwDesiredAccess: u32, bInheritHandle: i32, dwProcessId: u32) -> *mut c_void;
        fn GetExitCodeProcess(hProcess: *mut c_void, lpExitCode: *mut u32) -> i32;
        fn CloseHandle(hObject: *mut c_void) -> i32;
    }

    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        return false;
    }

    let mut exit_code = 0;
    let ok = unsafe { GetExitCodeProcess(handle, &mut exit_code) != 0 };
    unsafe {
        let _ = CloseHandle(handle);
    }
    ok && exit_code == STILL_ACTIVE
}

#[cfg(all(not(unix), not(windows)))]
fn platform_process_alive(_pid: u32) -> bool {
    false
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}
