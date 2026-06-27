//! CLI 侧 MCP daemon/proxy 启动逻辑。
//!
//! 普通 `serve --mcp` 会优先连接项目共享 daemon；连接不上再拉起后台进程，
//! 最后才回退到当前进程内服务，确保编辑器启动路径既快又不因 daemon 问题失效。

mod runtime;
mod socket;

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

use rustcodegraph::directory::get_code_graph_dir;
use rustcodegraph::mcp::daemon_paths::get_daemon_socket_path;
use rustcodegraph::mcp::proxy::{
    HOST_PPID_ENV, client_hello_line, connect_with_hello_result, log_attached_daemon,
};

use super::args::{command_path_arg, find_nearest_sqlite_root, path_option, resolve_project_path};
use super::mcp::{current_parent_pid, run_mcp_stdio};
use socket::{
    LocalStream, connect_local_stream, connection_failure_is_unavailable, daemon_socket_may_exist,
    detach_command, read_limited_line,
};

const RUST_DAEMON_INTERNAL_ENV: &str = "RUSTCODEGRAPH_DAEMON_INTERNAL";
const TAKEOVER_MAX_RETRIES: usize = 5;
const TAKEOVER_RETRY_DELAY_MS: u64 = 100;
const DAEMON_CONNECT_MAX_RETRIES: usize = 240;
const DAEMON_CONNECT_RETRY_DELAY_MS: u64 = 25;

pub(crate) fn command_serve(args: &[String]) -> Result<(), String> {
    let requested_path = path_option(args).or_else(|| command_path_arg(args));
    let project_root = resolve_project_path(requested_path);
    let watch_enabled = !args.iter().any(|arg| arg == "--no-watch");

    if daemon_internal_set_cli() {
        // 内部环境变量只由 spawn_detached_daemon 设置；避免用户命令再次进入 proxy 分支递归拉起自己。
        let root = resolve_daemon_root_for_runtime(&project_root).unwrap_or(project_root);
        return runtime::run_daemon_process(root);
    }

    if !watch_enabled {
        // 显式 --no-watch 表示调用方想要一次性 stdio MCP，不参与共享 daemon 生命周期。
        return run_mcp_stdio(project_root, false);
    }

    if daemon_opt_out_set_cli() {
        return run_mcp_stdio(project_root, true);
    }

    let Some(root) = resolve_daemon_root_for_runtime(&project_root) else {
        return run_mcp_stdio(project_root, true);
    };

    run_proxy_or_direct(root)
}

fn daemon_opt_out_set_cli() -> bool {
    env_truthy(&["RUSTCODEGRAPH_NO_DAEMON"])
}

fn daemon_internal_set_cli() -> bool {
    env_truthy(&[RUST_DAEMON_INTERNAL_ENV])
}

fn env_truthy(names: &[&str]) -> bool {
    names.iter().any(|name| {
        std::env::var(name)
            .is_ok_and(|raw| !raw.is_empty() && raw != "0" && !raw.eq_ignore_ascii_case("false"))
    })
}

fn resolve_daemon_root_for_runtime(candidate: &Path) -> Option<PathBuf> {
    let root = find_nearest_sqlite_root(candidate)?;
    // daemon 必须绑定稳定绝对根路径，否则 socket 路径会随调用 cwd 变化而产生多个实例。
    Some(root.canonicalize().unwrap_or(root))
}

fn run_proxy_or_direct(root: PathBuf) -> Result<(), String> {
    let socket_path = get_daemon_socket_path(&root);
    match connect_to_matching_daemon(&socket_path)? {
        DaemonConnect::Ready(stream) => return runtime::run_proxy_session(root, stream),
        DaemonConnect::VersionMismatch => return run_mcp_stdio_with_notice(root),
        DaemonConnect::Unavailable => {}
    }

    spawn_detached_daemon(&root)?;
    // 后台 daemon 需要完成 socket bind 和 hello 写入；短间隔轮询比固定长 sleep 更快响应。
    for _ in 0..DAEMON_CONNECT_MAX_RETRIES {
        thread::sleep(Duration::from_millis(DAEMON_CONNECT_RETRY_DELAY_MS));
        match connect_to_matching_daemon(&socket_path)? {
            DaemonConnect::Ready(stream) => return runtime::run_proxy_session(root, stream),
            DaemonConnect::VersionMismatch => return run_mcp_stdio_with_notice(root),
            DaemonConnect::Unavailable => {}
        }
    }

    eprintln!(
        "[RustCodeGraph MCP] Shared daemon did not become ready; serving this session in-process."
    );
    run_mcp_stdio(root, true)
}

fn run_mcp_stdio_with_notice(root: PathBuf) -> Result<(), String> {
    eprintln!(
        "[RustCodeGraph MCP] Shared daemon unavailable or incompatible; serving this session in-process."
    );
    run_mcp_stdio(root, true)
}

enum DaemonConnect {
    Ready(LocalStream),
    VersionMismatch,
    Unavailable,
}

fn connect_to_matching_daemon(socket_path: &Path) -> Result<DaemonConnect, String> {
    if !daemon_socket_may_exist(socket_path) {
        return Ok(DaemonConnect::Unavailable);
    }

    let mut stream = match connect_local_stream(socket_path) {
        Ok(stream) => stream,
        Err(err) if connection_failure_is_unavailable(&err) => {
            return Ok(DaemonConnect::Unavailable);
        }
        Err(err) => return Err(format!("failed to connect to daemon socket: {err}")),
    };
    let _ = stream.set_read_timeout(Some(Duration::from_secs(3)));

    // hello 是版本兼容与协议健康检查；坏 socket/旧 daemon 都会在这里被降级成不可用。
    let hello_line = match read_limited_line(&mut stream, 4096, Duration::from_secs(3)) {
        Ok(Some(line)) => line,
        Ok(None) => return Ok(DaemonConnect::Unavailable),
        Err(err) => return Err(err),
    };

    match connect_with_hello_result(&hello_line, Some(env!("CARGO_PKG_VERSION"))) {
        Ok(Some(hello)) => {
            let _ = stream.set_read_timeout(None);
            log_attached_daemon(socket_path, &hello);
            let host_ppid_raw = std::env::var(HOST_PPID_ENV).ok();
            let current_ppid = current_parent_pid().unwrap_or(0);
            // 把宿主和当前父进程都告诉 daemon，用于后续 liveness/PPID watchdog 判断。
            stream
                .write_all(client_hello_line(host_ppid_raw.as_deref(), current_ppid).as_bytes())
                .map_err(|err| format!("failed to send daemon client hello: {err}"))?;
            Ok(DaemonConnect::Ready(stream))
        }
        Ok(None) => {
            eprintln!(
                "[RustCodeGraph MCP] Found a daemon on {} but its version differs from ours; serving this session in-process.",
                socket_path.display()
            );
            Ok(DaemonConnect::VersionMismatch)
        }
        Err(_) => Ok(DaemonConnect::Unavailable),
    }
}

fn spawn_detached_daemon(root: &Path) -> Result<(), String> {
    let exe =
        std::env::current_exe().map_err(|err| format!("cannot resolve current exe: {err}"))?;
    let log_path = get_code_graph_dir(root).join("daemon.log");
    let log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|err| format!("failed to open daemon log {}: {err}", log_path.display()))?;
    let log_for_stderr = log
        .try_clone()
        .map_err(|err| format!("failed to clone daemon log handle: {err}"))?;

    let mut command = Command::new(exe);
    command
        .args(["serve", "--mcp", "--path"])
        .arg(root)
        .current_dir(std::env::temp_dir())
        .stdin(Stdio::null())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(log_for_stderr))
        .env(RUST_DAEMON_INTERNAL_ENV, "1");

    // 平台差异封装在 socket::detach_command：Unix/Windows 的 detached 语义不能混在主流程里。
    detach_command(&mut command);

    let child = command
        .spawn()
        .map_err(|err| format!("failed to spawn shared daemon: {err}"))?;
    drop(child);
    Ok(())
}
