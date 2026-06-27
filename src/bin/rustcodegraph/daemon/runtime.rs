//! CLI daemon runtime：把多个 MCP stdio 会话复用到同一个项目级 daemon。
//!
//! 这里维护两条路径：优先把客户端消息转发给共享 daemon；如果 daemon 中途断开，
//! 当前 stdio 会话立刻退回到进程内 MCP session，避免宿主 agent 因一次 socket 故障
//! 失去 rustcodegraph 工具。

use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use std::thread;
use std::time::{Duration, Instant};

use rustcodegraph::mcp::daemon::{
    AcquireResult, ClientPeerPids, clear_stale_daemon_lock, daemon_hello, is_process_alive,
    parse_client_hello_line, resolve_client_sweep_ms, resolve_idle_timeout_ms, resolve_max_idle_ms,
    try_acquire_daemon_lock,
};
use rustcodegraph::mcp::daemon_paths::{
    decode_lock_info, get_daemon_pid_path, get_daemon_socket_path,
};
#[cfg(windows)]
use rustcodegraph::mcp::daemon_paths::daemon_loopback_addr;
use serde_json::Value;

use super::socket::{LocalListener, LocalStream};
use super::{TAKEOVER_MAX_RETRIES, TAKEOVER_RETRY_DELAY_MS};
use crate::rustcodegraph_cli::mcp::{
    McpStdioSession, handle_mcp_message, install_mcp_ppid_watchdog, read_mcp_wire_message,
    write_mcp_wire_message,
};

pub(super) fn run_proxy_session(
    root: PathBuf,
    mut daemon_stream: LocalStream,
) -> Result<(), String> {
    let _watchdog = install_mcp_ppid_watchdog();
    let daemon_alive = Arc::new(AtomicBool::new(true));
    let shutting_down = Arc::new(AtomicBool::new(false));
    let stdout = Arc::new(Mutex::new(io::stdout()));

    // daemon -> stdout 独立线程负责把共享 daemon 的响应原样送回宿主。stdout
    // 用互斥锁保护，因为 fallback session 也可能在同一进程里写响应。
    let daemon_reader_stream = daemon_stream
        .try_clone()
        .map_err(|err| format!("failed to clone daemon socket: {err}"))?;
    let daemon_alive_for_reader = Arc::clone(&daemon_alive);
    let shutting_down_for_reader = Arc::clone(&shutting_down);
    let stdout_for_reader = Arc::clone(&stdout);
    thread::spawn(move || {
        let mut reader = io::BufReader::new(daemon_reader_stream);
        loop {
            match read_mcp_wire_message(&mut reader) {
                Ok(Some(message)) => {
                    if let Ok(mut out) = stdout_for_reader.lock() {
                        let _ = write_mcp_wire_message(&mut *out, &message.value, message.wire);
                    }
                }
                Ok(None) => break,
                Err(err) => {
                    eprintln!("[RustCodeGraph MCP] Daemon socket read failed ({err}).");
                    break;
                }
            }
        }
        daemon_alive_for_reader.store(false, Ordering::SeqCst);
        if !shutting_down_for_reader.load(Ordering::SeqCst) {
            eprintln!(
                "[RustCodeGraph MCP] Shared daemon disconnected; serving this session in-process."
            );
        }
    });

    // stdin 主循环只做一件事：daemon 可用时转发；一旦写失败或读线程发现断线，
    // 立即用同一条 stdio 连接继续服务，保持 MCP request/response 连续。
    let stdin = io::stdin();
    let mut reader = io::BufReader::new(stdin.lock());
    let mut fallback_session = McpStdioSession::new(root, true);

    while let Some(message) = read_mcp_wire_message(&mut reader)? {
        if daemon_alive.load(Ordering::SeqCst) {
            if write_mcp_wire_message(&mut daemon_stream, &message.value, message.wire).is_ok() {
                continue;
            }
            daemon_alive.store(false, Ordering::SeqCst);
            eprintln!(
                "[RustCodeGraph MCP] Shared daemon write failed; serving this session in-process."
            );
        }

        let mut out = stdout
            .lock()
            .map_err(|_| "stdout lock poisoned while serving fallback".to_string())?;
        handle_mcp_message(
            &mut fallback_session,
            &mut reader,
            &mut *out,
            message.value,
            message.wire,
        )
        .inspect_err(|_| fallback_session.stop_watcher())?;
    }

    shutting_down.store(true, Ordering::SeqCst);
    fallback_session.stop_watcher();
    Ok(())
}

#[derive(Debug)]
struct RuntimeDaemonState {
    project_root: PathBuf,
    socket_path: PathBuf,
    pid_path: PathBuf,
    // 客户端先注册连接，再通过内部 hello 补上真实宿主 PID；因此值允许暂时为空。
    clients: Mutex<BTreeMap<usize, Option<ClientPeerPids>>>,
    next_client_id: AtomicUsize,
    // idle timeout 只从“没有客户端”时开始计时，避免长会话被普通空闲间隙误杀。
    zero_clients_since: Mutex<Option<Instant>>,
    // max-idle 是有客户端但完全无请求时的兜底，防止宿主断线但 PID 仍短暂存活。
    last_activity_at: Mutex<Instant>,
    stopping: AtomicBool,
}

impl RuntimeDaemonState {
    fn new(project_root: PathBuf) -> Self {
        let now = Instant::now();
        Self {
            socket_path: get_daemon_socket_path(&project_root),
            pid_path: get_daemon_pid_path(&project_root),
            project_root,
            clients: Mutex::new(BTreeMap::new()),
            next_client_id: AtomicUsize::new(1),
            zero_clients_since: Mutex::new(Some(now)),
            last_activity_at: Mutex::new(now),
            stopping: AtomicBool::new(false),
        }
    }

    fn add_client(&self) -> usize {
        let id = self.next_client_id.fetch_add(1, Ordering::SeqCst);
        if let Ok(mut clients) = self.clients.lock() {
            clients.insert(id, None);
        }
        if let Ok(mut zero_since) = self.zero_clients_since.lock() {
            *zero_since = None;
        }
        self.record_activity();
        id
    }

    fn set_client_peer(&self, id: usize, peers: ClientPeerPids) {
        if let Ok(mut clients) = self.clients.lock()
            && let Some(slot) = clients.get_mut(&id)
        {
            *slot = Some(peers);
        }
    }

    fn remove_client(&self, id: usize) {
        let is_empty = if let Ok(mut clients) = self.clients.lock() {
            clients.remove(&id);
            clients.is_empty()
        } else {
            false
        };
        if is_empty && let Ok(mut zero_since) = self.zero_clients_since.lock() {
            *zero_since = Some(Instant::now());
        }
    }

    fn client_count(&self) -> usize {
        self.clients
            .lock()
            .map(|clients| clients.len())
            .unwrap_or(0)
    }

    fn record_activity(&self) {
        if let Ok(mut last) = self.last_activity_at.lock() {
            *last = Instant::now();
        }
    }

    fn cleanup_lockfile(&self) {
        let Ok(raw) = fs::read_to_string(&self.pid_path) else {
            return;
        };
        let Some(info) = decode_lock_info(&raw) else {
            return;
        };
        // 只删除仍指向自己的锁文件，避免竞态中误删新 daemon 已经写入的锁。
        if info.pid == std::process::id() {
            let _ = fs::remove_file(&self.pid_path);
        }
    }

    fn reap_dead_clients(&self) -> usize {
        // 客户端 PID 和宿主 PID 任一死亡，都说明这条 MCP 连接已经不可信；
        // 主循环用这个结果推进 zero_clients_since，释放无人使用的 daemon。
        let dead = if let Ok(clients) = self.clients.lock() {
            clients
                .iter()
                .filter_map(|(id, peers)| {
                    let peers = peers.as_ref()?;
                    let client_dead = peers.pid.is_some_and(|pid| !is_process_alive(pid));
                    let host_dead = peers.host_pid.is_some_and(|pid| !is_process_alive(pid));
                    (client_dead || host_dead).then_some(*id)
                })
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };

        for id in &dead {
            self.remove_client(*id);
        }
        dead.len()
    }
}

pub(super) fn run_daemon_process(root: PathBuf) -> Result<(), String> {
    for _ in 0..TAKEOVER_MAX_RETRIES {
        match try_acquire_daemon_lock(&root) {
            AcquireResult::Acquired { .. } => return run_bound_daemon(root),
            AcquireResult::Taken { existing, pid_path } => {
                if existing
                    .as_ref()
                    .is_some_and(|info| info.pid > 0 && is_process_alive(info.pid))
                {
                    eprintln!(
                        "[RustCodeGraph daemon] Another daemon (pid {}) already holds the lock; exiting.",
                        existing.as_ref().map(|info| info.pid).unwrap_or(0)
                    );
                    return Ok(());
                }
                // stale lock 可能来自被 SIGKILL 的旧进程；短暂 sleep 让并发启动者重新竞争。
                let _ = clear_stale_daemon_lock(pid_path, existing.map(|info| info.pid));
                thread::sleep(Duration::from_millis(TAKEOVER_RETRY_DELAY_MS));
            }
        }
    }
    Err("could not acquire daemon lock".to_string())
}

fn run_bound_daemon(root: PathBuf) -> Result<(), String> {
    let state = Arc::new(RuntimeDaemonState::new(root));
    let listener = bind_daemon_listener(&state.socket_path)?;
    eprintln!(
        "[RustCodeGraph daemon] Listening on {} (pid {}, v{}). Idle timeout {}ms.",
        state.socket_path.display(),
        std::process::id(),
        env!("CARGO_PKG_VERSION"),
        resolve_idle_timeout_ms()
    );

    let idle_timeout_ms = resolve_idle_timeout_ms();
    let max_idle_ms = resolve_max_idle_ms();
    let sweep_ms = resolve_client_sweep_ms();
    let mut last_sweep = Instant::now();

    loop {
        match accept_daemon_client(&listener) {
            Ok(Some(stream)) => {
                let client_state = Arc::clone(&state);
                thread::spawn(move || {
                    handle_daemon_client(client_state, stream);
                });
            }
            Ok(None) => {}
            Err(err) => {
                state.stopping.store(true, Ordering::SeqCst);
                state.cleanup_lockfile();
                return Err(format!("daemon accept failed: {err}"));
            }
        }

        // 周期性主动清扫死客户端；这比等待 socket 读失败更快覆盖宿主崩溃场景。
        if sweep_ms > 0 && last_sweep.elapsed() >= Duration::from_millis(sweep_ms) {
            let reaped = state.reap_dead_clients();
            if reaped > 0 {
                eprintln!("[RustCodeGraph daemon] Reaped {reaped} dead client(s).");
            }
            last_sweep = Instant::now();
        }

        // 没有客户端时按 idle timeout 退出，留下的 socket/lock 会在退出路径清理。
        if idle_timeout_ms > 0 {
            let idle_elapsed = state
                .zero_clients_since
                .lock()
                .ok()
                .and_then(|since| since.map(|instant| instant.elapsed()))
                .unwrap_or_default();
            if state.client_count() == 0 && idle_elapsed >= Duration::from_millis(idle_timeout_ms) {
                eprintln!("[RustCodeGraph daemon] Stopping after idle timeout.");
                break;
            }
        }

        // 有客户端但长时间没有任何 MCP 消息时，max-idle 防止 daemon 被半开连接拖住。
        if max_idle_ms > 0 && state.client_count() > 0 {
            let inactive_for = state
                .last_activity_at
                .lock()
                .map(|last| last.elapsed())
                .unwrap_or_default();
            if inactive_for >= Duration::from_millis(max_idle_ms) {
                eprintln!("[RustCodeGraph daemon] Stopping after inactivity backstop.");
                break;
            }
        }

        if state.stopping.load(Ordering::SeqCst) {
            break;
        }

        thread::sleep(Duration::from_millis(25));
    }

    state.stopping.store(true, Ordering::SeqCst);
    state.cleanup_lockfile();
    cleanup_daemon_socket(&state.socket_path);
    Ok(())
}

#[cfg(unix)]
fn bind_daemon_listener(socket_path: &Path) -> Result<LocalListener, String> {
    let _ = fs::remove_file(socket_path);
    if let Some(parent) = socket_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create socket dir {}: {err}", parent.display()))?;
    }
    let listener = LocalListener::bind(socket_path).map_err(|err| {
        format!(
            "failed to bind daemon socket {}: {err}",
            socket_path.display()
        )
    })?;
    listener
        .set_nonblocking(true)
        .map_err(|err| format!("failed to set daemon listener nonblocking: {err}"))?;
    // Unix domain socket 放在项目私有目录下，并收紧权限，避免同机其他用户接入会话。
    let _ = fs::set_permissions(socket_path, fs::Permissions::from_mode(0o600));
    Ok(listener)
}

#[cfg(windows)]
fn bind_daemon_listener(socket_path: &Path) -> Result<LocalListener, String> {
    let addr = daemon_loopback_addr(socket_path);
    let listener = LocalListener::bind(addr)
        .map_err(|err| format!("failed to bind Windows daemon loopback {addr}: {err}"))?;
    listener
        .set_nonblocking(true)
        .map_err(|err| format!("failed to set Windows daemon listener nonblocking: {err}"))?;
    Ok(listener)
}

#[cfg(all(not(unix), not(windows)))]
fn bind_daemon_listener(_socket_path: &Path) -> Result<LocalListener, String> {
    Err("local daemon sockets are not implemented on this platform".to_string())
}

#[cfg(unix)]
fn accept_daemon_client(listener: &LocalListener) -> io::Result<Option<LocalStream>> {
    match listener.accept() {
        Ok((stream, _addr)) => {
            let _ = stream.set_nonblocking(false);
            Ok(Some(stream))
        }
        Err(err) if err.kind() == io::ErrorKind::WouldBlock => Ok(None),
        Err(err) => Err(err),
    }
}

#[cfg(windows)]
fn accept_daemon_client(listener: &LocalListener) -> io::Result<Option<LocalStream>> {
    match listener.accept() {
        Ok((stream, _addr)) => Ok(Some(stream)),
        Err(err) if err.kind() == io::ErrorKind::WouldBlock => Ok(None),
        Err(err) => Err(err),
    }
}

#[cfg(all(not(unix), not(windows)))]
fn accept_daemon_client(_listener: &LocalListener) -> io::Result<Option<LocalStream>> {
    Ok(None)
}

#[cfg(unix)]
fn cleanup_daemon_socket(socket_path: &Path) {
    let _ = fs::remove_file(socket_path);
}

#[cfg(not(unix))]
fn cleanup_daemon_socket(_socket_path: &Path) {}

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

fn handle_daemon_client(state: Arc<RuntimeDaemonState>, mut stream: LocalStream) {
    let client_id = state.add_client();
    let hello = daemon_hello(&state.socket_path);
    if let Ok(mut line) = serde_json::to_string(&hello) {
        line.push('\n');
        if stream.write_all(line.as_bytes()).is_err() {
            state.remove_client(client_id);
            return;
        }
    }

    let reader_stream = match stream.try_clone() {
        Ok(stream) => stream,
        Err(_) => {
            state.remove_client(client_id);
            return;
        }
    };
    let mut reader = io::BufReader::new(reader_stream);
    let mut session = McpStdioSession::new(state.project_root.clone(), true);

    loop {
        match read_mcp_wire_message(&mut reader) {
            Ok(Some(message)) => {
                // rustcodegraph_client 是代理层内部 hello，不转给 MCP handler；它只用来
                // 记录客户端/宿主 PID，方便 daemon 主循环回收失联连接。
                if message
                    .value
                    .get("rustcodegraph_client")
                    .and_then(Value::as_u64)
                    == Some(1)
                {
                    if let Some(peers) = client_peers_from_value(&message.value) {
                        state.set_client_peer(client_id, peers);
                    }
                    continue;
                }

                state.record_activity();
                if handle_mcp_message(
                    &mut session,
                    &mut reader,
                    &mut stream,
                    message.value,
                    message.wire,
                )
                .is_err()
                {
                    break;
                }
            }
            Ok(None) => break,
            Err(err) => {
                eprintln!("[RustCodeGraph daemon] Client read failed ({err}).");
                break;
            }
        }
    }

    session.stop_watcher();
    state.remove_client(client_id);
}

fn client_peers_from_value(value: &Value) -> Option<ClientPeerPids> {
    serde_json::to_string(value)
        .ok()
        .and_then(|line| parse_client_hello_line(&line))
}
