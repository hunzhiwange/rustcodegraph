//! Per-connection MCP session state translated from `session.ts`.
//!
//! session 负责 MCP initialize/tools/list 的项目发现。未索引时返回 inactive
//! instructions 和空 tools，避免 agent 反复调用会失败的工具。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::directory::find_nearest_code_graph_root;

use super::engine::MCPEngine;
use super::server_instructions::{SERVER_INSTRUCTIONS, SERVER_INSTRUCTIONS_UNINDEXED};
use super::tools::tools;
use super::transport::{JsonRpcRequest, JsonRpcTransport, error_codes};
use super::version::code_graph_package_version;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerInfo {
    pub name: String,
    pub version: String,
}

pub const PROTOCOL_VERSION: &str = "2024-11-05";
pub const ROOTS_LIST_TIMEOUT_MS: u64 = 5_000;

pub fn server_info() -> ServerInfo {
    ServerInfo {
        name: "rustcodegraph".to_string(),
        version: code_graph_package_version().to_string(),
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MCPSessionOptions {
    pub explicit_project_path: Option<PathBuf>,
}

pub struct MCPSession<T: JsonRpcTransport> {
    transport: T,
    engine: MCPEngine,
    client_supports_roots: bool,
    roots_attempted: bool,
    explicit_project_path: Option<PathBuf>,
}

impl<T: JsonRpcTransport> MCPSession<T> {
    pub fn new(transport: T, engine: MCPEngine, opts: Option<MCPSessionOptions>) -> Self {
        Self {
            transport,
            engine,
            client_supports_roots: false,
            roots_attempted: false,
            explicit_project_path: opts.and_then(|opts| opts.explicit_project_path),
        }
    }

    pub fn start(&mut self) {
        self.transport.start();
    }

    pub fn stop(&mut self) {
        self.transport.stop();
    }

    pub fn handle_initialize(&mut self, request: &JsonRpcRequest) {
        // rootUri 优先，其次 workspaceFolders，再其次显式 CLI path；不同宿主
        // 对 MCP roots 支持不一致，所以这里要宽松接受多种来源。
        let params = request.params.as_ref();
        self.client_supports_roots = params
            .and_then(|p| p.get("capabilities"))
            .and_then(|c| c.get("roots"))
            .is_some();

        let explicit = params
            .and_then(|p| p.get("rootUri"))
            .and_then(Value::as_str)
            .map(file_uri_to_path)
            .or_else(|| {
                params
                    .and_then(|p| p.get("workspaceFolders"))
                    .and_then(Value::as_array)
                    .and_then(|folders| folders.first())
                    .and_then(|first| first.get("uri"))
                    .and_then(Value::as_str)
                    .map(file_uri_to_path)
            })
            .or_else(|| self.explicit_project_path.clone());

        let candidate = explicit.clone().or_else(|| std::env::current_dir().ok());
        let indexed = candidate
            .as_ref()
            .and_then(find_nearest_code_graph_root)
            .is_some();

        self.transport.send_result(
            request.id.clone(),
            json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": { "tools": {} },
                "serverInfo": server_info(),
                "instructions": if indexed { SERVER_INSTRUCTIONS } else { SERVER_INSTRUCTIONS_UNINDEXED },
            }),
        );

        if let Some(path) = explicit {
            self.engine.ensure_initialized(path);
        }
    }

    pub fn handle_tools_list(&mut self, request: &JsonRpcRequest) {
        // tools/list 是第二次初始化机会：有些客户端 initialize 不带 roots，
        // 但当前 cwd 或显式 hint 已经足够找到 `.rustcodegraph/`。
        self.retry_init_if_needed();
        let listed = if self.engine.has_default_code_graph() {
            self.engine.get_tool_handler().get_tools()
        } else {
            Vec::new()
        };
        self.transport
            .send_result(request.id.clone(), json!({ "tools": listed }));
    }

    pub fn handle_unknown_request(&mut self, request: &JsonRpcRequest) {
        self.transport.send_error(
            request.id.clone(),
            error_codes::METHOD_NOT_FOUND,
            &format!("Method not found: {}", request.method),
            None,
        );
    }

    pub fn static_tools_list() -> Value {
        json!({ "tools": tools() })
    }

    fn retry_init_if_needed(&mut self) {
        if self.engine.has_default_code_graph() {
            return;
        }
        let hint = self
            .explicit_project_path
            .clone()
            .or_else(|| self.engine.get_project_path().map(Path::to_path_buf))
            .or_else(|| std::env::current_dir().ok());
        if let Some(hint) = hint {
            self.engine.retry_initialize_sync(hint);
        }
        self.roots_attempted = true;
    }
}

pub fn file_uri_to_path(uri: &str) -> PathBuf {
    // MCP roots 使用 file URI；Windows 盘符会表现为 `/C:/...`，需要去掉
    // 前导 slash 才能得到本地路径。
    let without_scheme = uri.strip_prefix("file://").unwrap_or(uri);
    let decoded = percent_decode(without_scheme);
    #[cfg(windows)]
    {
        if decoded.as_bytes().get(0) == Some(&b'/') && decoded.as_bytes().get(2) == Some(&b':') {
            return PathBuf::from(&decoded[1..]);
        }
    }
    PathBuf::from(decoded)
}

pub fn first_root_path(result: &Value) -> Option<PathBuf> {
    result
        .get("roots")
        .and_then(Value::as_array)
        .and_then(|roots| roots.first())
        .and_then(|first| first.get("uri"))
        .and_then(Value::as_str)
        .map(file_uri_to_path)
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(hex) = u8::from_str_radix(&input[i + 1..i + 3], 16)
        {
            out.push(hex);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}
