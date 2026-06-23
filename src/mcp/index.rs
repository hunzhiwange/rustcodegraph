//! CodeGraph MCP server entrypoint translated from `index.ts`.
//!
//! MCP server 启动分三种模式：direct、proxy、daemon。这里先做纯决策和
//! 生命周期挂钩，具体 socket/stdio 运行时由相邻模块承载。

use std::env;
use std::path::{Path, PathBuf};

use crate::directory::find_nearest_code_graph_root;

use super::daemon::{
    AcquireResult, Daemon, clear_stale_daemon_lock, is_process_alive, try_acquire_daemon_lock,
};
use super::engine::MCPEngine;
use super::liveness_watchdog::{DeferredWatchdogHandle, install_main_thread_watchdog};
use super::ppid_watchdog::{SupervisionState, supervision_lost_reason};
use super::proxy::{HOST_PPID_ENV, run_proxy};
use super::stdin_teardown::{StdinTeardownGuard, treat_stdin_failure_as_shutdown};
use super::transport::StdioTransport;

pub const DEFAULT_PPID_POLL_MS: u64 = 5_000;
pub const DAEMON_INTERNAL_ENV: &str = "RUSTCODEGRAPH_DAEMON_INTERNAL";
pub const TAKEOVER_MAX_RETRIES: usize = 5;
pub const TAKEOVER_RETRY_DELAY_MS: u64 = 100;
pub const DAEMON_CONNECT_MAX_RETRIES: usize = 240;
pub const DAEMON_CONNECT_RETRY_DELAY_MS: u64 = 25;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MCPServerMode {
    Unstarted,
    Direct,
    Proxy,
    Daemon,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartOutcome {
    Direct { reason: String },
    Proxy { root: PathBuf },
    Daemon { root: PathBuf },
}

#[derive(Debug)]
pub struct MCPServer {
    project_path: Option<PathBuf>,
    engine: Option<MCPEngine>,
    daemon: Option<Daemon>,
    mode: MCPServerMode,
    original_ppid: u32,
    host_ppid: Option<u32>,
    stopped: bool,
    liveness_watchdog: Option<DeferredWatchdogHandle>,
    stdin_guard: Option<StdinTeardownGuard>,
}

impl MCPServer {
    pub fn new(project_path: Option<PathBuf>) -> Self {
        Self {
            project_path,
            engine: None,
            daemon: None,
            mode: MCPServerMode::Unstarted,
            original_ppid: 0,
            host_ppid: parse_host_ppid(env::var(HOST_PPID_ENV).ok().as_deref()),
            stopped: false,
            liveness_watchdog: None,
            stdin_guard: None,
        }
    }

    /// Decide the runtime mode without starting processes or sockets.
    pub fn start(&mut self) -> StartOutcome {
        // daemon 内部子进程不能再走 proxy 逻辑，否则会递归启动自己。
        if daemon_internal_set() {
            let root = resolve_daemon_root(self.project_path.as_deref())
                .or_else(|| self.project_path.clone())
                .or_else(|| env::current_dir().ok())
                .unwrap_or_else(|| PathBuf::from("."));
            self.mode = MCPServerMode::Daemon;
            self.daemon = Some(Daemon::new(&root, None));
            self.liveness_watchdog = install_main_thread_watchdog();
            return StartOutcome::Daemon { root };
        }

        if daemon_opt_out_set() {
            return self.start_direct("RUSTCODEGRAPH_NO_DAEMON set");
        }

        let Some(root) = resolve_daemon_root(self.project_path.as_deref()) else {
            // 没有 `.rustcodegraph/` 时直接服务一个空工具列表/未索引说明；
            // 索引是用户决定，不在 MCP 启动时自动创建。
            return self.start_direct("no .rustcodegraph/ root found");
        };

        self.mode = MCPServerMode::Proxy;
        StartOutcome::Proxy { root }
    }

    pub fn stop(&mut self) {
        if self.stopped {
            return;
        }
        self.stopped = true;
        if let Some(engine) = &mut self.engine {
            engine.stop();
        }
        if let Some(daemon) = &mut self.daemon {
            daemon.stop("stop()");
        }
        if let Some(watchdog) = self.liveness_watchdog.take() {
            watchdog.stop();
        }
        self.mode = MCPServerMode::Unstarted;
    }

    pub fn mode(&self) -> MCPServerMode {
        self.mode
    }

    pub fn supervision_lost(&self, current_ppid: u32) -> Option<String> {
        supervision_lost_reason(SupervisionState {
            original_ppid: self.original_ppid,
            current_ppid,
            host_ppid: self.host_ppid,
            is_alive: is_process_alive,
            platform: None,
        })
    }

    fn start_direct(&mut self, reason: &str) -> StartOutcome {
        self.mode = MCPServerMode::Direct;
        self.engine = Some(MCPEngine::new(None));
        self.stdin_guard = Some(treat_stdin_failure_as_shutdown(|| {}));
        self.liveness_watchdog = install_main_thread_watchdog();
        let _transport = StdioTransport::default();
        StartOutcome::Direct {
            reason: reason.to_string(),
        }
    }
}

pub fn parse_ppid_poll_ms(raw: Option<&str>) -> u64 {
    let Some(raw) = raw else {
        return DEFAULT_PPID_POLL_MS;
    };
    if raw.is_empty() {
        return DEFAULT_PPID_POLL_MS;
    }
    raw.parse::<u64>().unwrap_or(DEFAULT_PPID_POLL_MS)
}

pub fn parse_host_ppid(raw: Option<&str>) -> Option<u32> {
    let parsed = raw?.parse::<u32>().ok()?;
    (parsed > 1).then_some(parsed)
}

pub fn daemon_opt_out_set() -> bool {
    env_truthy(&["RUSTCODEGRAPH_NO_DAEMON"])
}

pub fn daemon_internal_set() -> bool {
    env_truthy(&[DAEMON_INTERNAL_ENV])
}

fn env_truthy(names: &[&str]) -> bool {
    names.iter().any(|name| {
        env::var(name)
            .is_ok_and(|raw| !raw.is_empty() && raw != "0" && !raw.eq_ignore_ascii_case("false"))
    })
}

pub fn resolve_daemon_root(explicit_path: Option<&Path>) -> Option<PathBuf> {
    let candidate = explicit_path
        .map(Path::to_path_buf)
        .or_else(|| env::current_dir().ok())?;
    let root = find_nearest_code_graph_root(candidate)?;
    Some(root.canonicalize().unwrap_or(root))
}

pub fn start_daemon_process_once(root: impl AsRef<Path>) -> Option<Daemon> {
    // 只负责“尝试成为 daemon”；如果锁被活进程占用，调用方应连接现有 daemon。
    match try_acquire_daemon_lock(root.as_ref()) {
        AcquireResult::Acquired { .. } => Some(Daemon::new(root.as_ref(), None)),
        AcquireResult::Taken { existing, pid_path } => {
            if existing
                .as_ref()
                .is_some_and(|info| info.pid > 0 && is_process_alive(info.pid))
            {
                return None;
            }
            let _ = clear_stale_daemon_lock(pid_path, existing.map(|info| info.pid));
            None
        }
    }
}

pub fn proxy_fallback_result(socket_path: impl AsRef<Path>) -> super::proxy::ProxyResult {
    run_proxy(socket_path, None)
}

pub use super::daemon::Daemon as SharedDaemon;
pub use super::tools::{ToolHandler, tools};
pub use super::transport::StdioTransport as ExportedStdioTransport;
pub use super::version::code_graph_package_version;
