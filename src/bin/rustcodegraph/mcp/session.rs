//! 每条 MCP 连接的 session 状态。
//!
//! 一个 session 既要记住项目根目录和 watcher，也要处理 MCP roots 协商、父进程
//! 监督和首次工具调用前的增量 catch-up。

use std::collections::BTreeSet;
use std::io;
use std::path::{Path, PathBuf};

use rustcodegraph::mcp::daemon::is_process_alive;
use rustcodegraph::mcp::engine::parse_watch_policy_from_env;
use rustcodegraph::mcp::index::{parse_host_ppid, parse_ppid_poll_ms};
use rustcodegraph::mcp::ppid_watchdog::{SupervisionState, supervision_lost_reason};
use rustcodegraph::mcp::proxy::HOST_PPID_ENV;
use rustcodegraph::mcp::session::file_uri_to_path;
use serde_json::Value;

use super::handler::handle_mcp_message;
use super::wire::read_mcp_wire_message;
use crate::rustcodegraph_cli::args::resolve_project_path;
use crate::rustcodegraph_cli::storage::is_sqlite_initialized;

pub(crate) struct McpStdioSession {
    pub(super) project_root: PathBuf,
    pub(super) watch_enabled: bool,
    pub(super) client_supports_roots: bool,
    pub(super) roots_attempted: bool,
    pub(super) next_server_request_id: u64,
    // watcher 用 CodeGraph 持有底层文件监听；关闭 session 时必须显式 close。
    watcher: Option<rustcodegraph::CodeGraph>,
    // 同一项目每个 session 只主动 sync 一次，避免频繁工具调用反复扫文件系统。
    caught_up_roots: BTreeSet<PathBuf>,
}

impl McpStdioSession {
    pub(crate) fn new(project_root: PathBuf, watch_enabled: bool) -> Self {
        Self {
            project_root,
            watch_enabled,
            client_supports_roots: false,
            roots_attempted: false,
            next_server_request_id: 1,
            watcher: None,
            caught_up_roots: BTreeSet::new(),
        }
    }

    pub(super) fn capture_initialize_params(&mut self, params: Option<&Value>) {
        // rootUri/workspaceFolders 比启动 cwd 更可信；某些 host 会在 initialize 才提供真实项目根。
        self.client_supports_roots = params
            .and_then(|params| params.get("capabilities"))
            .and_then(|capabilities| capabilities.get("roots"))
            .is_some();

        if let Some(path) = explicit_initialize_path(params) {
            self.project_root = resolve_project_path(Some(path.to_string_lossy().into_owned()));
        }
    }

    pub(super) fn start_watcher(&mut self) -> bool {
        if !self.watch_enabled {
            return false;
        }
        if self.watcher.is_some() {
            return true;
        }
        if !is_sqlite_initialized(&self.project_root) {
            return false;
        }
        let Ok(mut graph) = rustcodegraph::CodeGraph::open_sync(&self.project_root) else {
            return false;
        };
        // 复用库层 watch 策略解析，保证 CLI MCP 和 daemon MCP 的环境变量语义一致。
        let started = graph.watch(parse_watch_policy_from_env());
        if started {
            self.watcher = Some(graph);
        }
        started
    }

    pub(crate) fn stop_watcher(&mut self) {
        if let Some(mut watcher) = self.watcher.take() {
            watcher.close();
        }
    }

    pub(super) fn next_roots_request_id(&mut self) -> String {
        let id = format!("roots/list:{}", self.next_server_request_id);
        self.next_server_request_id += 1;
        id
    }

    pub(super) fn catch_up_once(&mut self, project_root: &Path) {
        if !is_sqlite_initialized(project_root) {
            return;
        }
        // canonicalize 失败时仍用原路径做 key；网络卷或刚删除的目录不应让工具调用失败。
        let key = project_root
            .canonicalize()
            .unwrap_or_else(|_| project_root.to_path_buf());
        if !self.caught_up_roots.insert(key.clone()) {
            return;
        }
        let Ok(mut graph) = rustcodegraph::CodeGraph::open_sync(&key) else {
            return;
        };
        let result = graph.sync(rustcodegraph::IndexOptions::default());
        let changed = result.files_added + result.files_modified + result.files_removed;
        if changed > 0 {
            eprintln!("[RustCodeGraph MCP] Caught up {changed} file(s) changed since last run");
        }
        graph.close();
    }
}

fn explicit_initialize_path(params: Option<&Value>) -> Option<PathBuf> {
    params
        .and_then(|params| params.get("rootUri"))
        .and_then(Value::as_str)
        .map(file_uri_to_path)
        .or_else(|| {
            params
                .and_then(|params| params.get("workspaceFolders"))
                .and_then(Value::as_array)
                .and_then(|folders| folders.first())
                .and_then(|first| first.get("uri"))
                .and_then(Value::as_str)
                .map(file_uri_to_path)
        })
}

pub(crate) fn run_mcp_stdio(project_root: PathBuf, watch_enabled: bool) -> Result<(), String> {
    let _watchdog = install_mcp_ppid_watchdog();
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = io::BufReader::new(stdin.lock());
    let mut writer = stdout.lock();
    let mut session = McpStdioSession::new(project_root, watch_enabled);

    while let Some(message) = read_mcp_wire_message(&mut reader)? {
        handle_mcp_message(
            &mut session,
            &mut reader,
            &mut writer,
            message.value,
            message.wire,
        )
        .inspect_err(|_| session.stop_watcher())?;
    }
    session.stop_watcher();
    Ok(())
}

pub(crate) fn install_mcp_ppid_watchdog() -> Option<std::thread::JoinHandle<()>> {
    let original_ppid = current_parent_pid()?;
    let host_ppid = mcp_host_ppid();
    let poll_ms =
        parse_ppid_poll_ms(std::env::var("RUSTCODEGRAPH_PPID_POLL_MS").ok().as_deref()).max(1);

    // host 退出但 stdio 管道未立刻关闭时，watchdog 兜底杀掉 MCP server，避免后台进程泄漏。
    Some(std::thread::spawn(move || {
        loop {
            std::thread::sleep(std::time::Duration::from_millis(poll_ms));
            let Some(current_ppid) = current_parent_pid() else {
                continue;
            };
            let Some(reason) = supervision_lost_reason(SupervisionState {
                original_ppid,
                current_ppid,
                host_ppid,
                is_alive: is_process_alive,
                platform: None,
            }) else {
                continue;
            };

            eprintln!("[RustCodeGraph MCP] Parent process exited ({reason}); shutting down.");
            std::process::exit(0);
        }
    }))
}

fn mcp_host_ppid() -> Option<u32> {
    // proxy/daemon 模式会传入宿主 PID；单纯比较 direct parent 会被中间 shell/launcher 干扰。
    std::env::var(HOST_PPID_ENV)
        .ok()
        .and_then(|raw| parse_host_ppid(Some(&raw)))
}

#[cfg(unix)]
pub(crate) fn current_parent_pid() -> Option<u32> {
    unsafe extern "C" {
        fn getppid() -> i32;
    }

    // getppid 是只读 libc 调用；返回 1 通常意味着原父进程已退出并被 init 接管。
    let pid = unsafe { getppid() };
    (pid > 0).then_some(pid as u32)
}

#[cfg(windows)]
pub(crate) fn current_parent_pid() -> Option<u32> {
    None
}

#[cfg(all(not(unix), not(windows)))]
pub(crate) fn current_parent_pid() -> Option<u32> {
    None
}
