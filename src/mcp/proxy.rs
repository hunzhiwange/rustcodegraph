//! MCP proxy-mode helpers translated from `proxy.ts`.
//!
//! proxy 模式把一次 MCP stdio 会话转发给项目 daemon。握手先验证版本，
//! 版本不一致时回退本地处理，避免新旧二进制共享协议时互相误读。

use std::env;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::daemon::{DaemonClientHello, DaemonHello, MAX_HELLO_LINE_BYTES};
use super::server_instructions::SERVER_INSTRUCTIONS;
use super::session::{PROTOCOL_VERSION, server_info};
use super::tools::{ToolHandler, get_static_tools};
use super::version::code_graph_package_version;

pub const DEFAULT_PPID_POLL_MS: u64 = 5_000;
pub const HOST_PPID_ENV: &str = "RUSTCODEGRAPH_HOST_PPID";
pub const LOG_ATTACH_ENV: &str = "RUSTCODEGRAPH_MCP_LOG_ATTACH";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProxyOutcome {
    Proxied,
    FallbackNeeded,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProxyResult {
    pub outcome: ProxyOutcome,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

pub fn log_attached_daemon(socket_path: impl AsRef<Path>, hello: &DaemonHello) {
    if env::var(LOG_ATTACH_ENV).ok().as_deref() != Some("1") {
        return;
    }
    let _ = writeln!(
        std::io::stderr(),
        "[RustCodeGraph MCP] Attached to shared daemon on {} (pid {}, v{}).",
        socket_path.as_ref().display(),
        hello.pid,
        hello.rustcodegraph
    );
}

/// Runtime socket piping is deferred; this returns the same fallback shape.
pub fn run_proxy(_socket_path: impl AsRef<Path>, _expected_version: Option<&str>) -> ProxyResult {
    ProxyResult {
        outcome: ProxyOutcome::FallbackNeeded,
        reason: Some("Rust proxy socket runtime is not wired yet".to_string()),
    }
}

pub fn connect_with_hello_result(
    hello_line: &str,
    expected_version: Option<&str>,
) -> Result<Option<DaemonHello>, String> {
    // None 表示“连上了 daemon 但版本不匹配”，调用方可以启动/使用本地 fallback。
    let hello = read_hello_line_from_str(hello_line)?;
    let expected = match expected_version {
        Some(version) => version,
        None => code_graph_package_version(),
    };
    if hello.rustcodegraph != expected {
        return Ok(None);
    }
    Ok(Some(hello))
}

pub fn client_hello(host_ppid_env: Option<&str>, current_ppid: u32) -> DaemonClientHello {
    // hostPid 优先取宿主透传值；没有时用当前 PPID，供 daemon 清理孤儿 proxy。
    DaemonClientHello {
        rustcodegraph_client: 1,
        pid: std::process::id(),
        host_pid: parse_host_ppid(host_ppid_env).or(Some(current_ppid)),
    }
}

pub fn client_hello_line(host_ppid_env: Option<&str>, current_ppid: u32) -> String {
    let mut line = serde_json::to_string(&client_hello(host_ppid_env, current_ppid))
        .unwrap_or_else(|_| "{}".to_string());
    line.push('\n');
    line
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalHandshakeDeps {
    pub root: PathBuf,
}

pub fn local_initialize_response(id: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": { "tools": {} },
            "serverInfo": server_info(),
            "instructions": SERVER_INSTRUCTIONS,
        }
    })
}

pub fn local_tools_list_response(id: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": { "tools": get_static_tools() },
    })
}

pub fn local_fallback_tool_result(
    handler: &mut ToolHandler,
    name: &str,
    args: &serde_json::Map<String, Value>,
) -> Value {
    json!(handler.execute(name, args))
}

pub fn read_hello_line_from_str(line: &str) -> Result<DaemonHello, String> {
    if line.len() > MAX_HELLO_LINE_BYTES {
        return Err("daemon hello line exceeded size limit".to_string());
    }
    let parsed: DaemonHello =
        serde_json::from_str(line.trim()).map_err(|err| format!("daemon hello not JSON: {err}"))?;
    if parsed.rustcodegraph.is_empty() || parsed.pid == 0 {
        return Err("daemon hello missing required fields".to_string());
    }
    Ok(parsed)
}

pub fn parse_poll_ms(raw: Option<&str>) -> u64 {
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

pub fn host_ppid_from_env() -> Option<u32> {
    parse_host_ppid(env::var(HOST_PPID_ENV).ok().as_deref())
}
