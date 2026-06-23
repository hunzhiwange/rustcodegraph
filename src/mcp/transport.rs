//! MCP JSON-RPC transport types translated from `transport.ts`.
//!
//! This file preserves the newline-delimited JSON-RPC message shapes and the
//! transport trait surface. Concrete stdio/socket event-loop wiring is deferred
//! so importing the Rust module never starts readers, writers, or sockets.
//!
//! 这里是协议形状和可测试的行协议内核；真实 stdio/socket 事件循环由上层模块接线，
//! 避免单元测试或库导入时意外占用 stdin/stdout。

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum JsonRpcId {
    // JSON-RPC 允许 string/number/null id；错误响应在无法读取 id 时必须回 null。
    String(String),
    Number(i64),
    Null,
}

impl From<&str> for JsonRpcId {
    fn from(value: &str) -> Self {
        Self::String(value.to_string())
    }
}

impl From<String> for JsonRpcId {
    fn from(value: String) -> Self {
        Self::String(value)
    }
}

impl From<i64> for JsonRpcId {
    fn from(value: i64) -> Self {
        Self::Number(value)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: JsonRpcId,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JsonRpcNotification {
    pub jsonrpc: String,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: JsonRpcId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum JsonRpcMessage {
    Request(JsonRpcRequest),
    Notification(JsonRpcNotification),
    Response(JsonRpcResponse),
}

pub mod error_codes {
    pub const PARSE_ERROR: i64 = -32700;
    pub const INVALID_REQUEST: i64 = -32600;
    pub const METHOD_NOT_FOUND: i64 = -32601;
    pub const INVALID_PARAMS: i64 = -32602;
    pub const INTERNAL_ERROR: i64 = -32603;
}

pub trait JsonRpcTransport {
    // transport trait 保持最小能力面：上层只关心发结果/错误/通知，具体写到
    // stdio 还是 socket 由实现决定。
    fn start(&mut self) {}
    fn stop(&mut self);
    fn send(&mut self, response: JsonRpcResponse);
    fn notify(&mut self, method: &str, params: Option<Value>);
    fn send_result(&mut self, id: JsonRpcId, result: Value);
    fn send_error(&mut self, id: JsonRpcId, code: i64, message: &str, data: Option<Value>);
}

#[derive(Debug, Default, Clone)]
pub struct LineBasedJsonRpcTransport {
    pub pending: HashMap<JsonRpcIdKey, String>,
    pub next_request_id: u64,
    pub stopped: bool,
    pub sent_lines: Vec<String>,
}

pub type JsonRpcIdKey = String;

impl LineBasedJsonRpcTransport {
    pub fn new() -> Self {
        Self {
            next_request_id: 1,
            ..Self::default()
        }
    }

    pub fn parse_line(&self, line: &str) -> Result<Option<JsonRpcMessage>, Box<JsonRpcResponse>> {
        // MCP stdio/socket 都是一行一个 JSON-RPC message；空行忽略，协议错误则返回
        // 可直接写回客户端的 JSON-RPC error response。
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }

        let parsed: Value = match serde_json::from_str(trimmed) {
            Ok(value) => value,
            Err(_) => {
                return Err(boxed_error_response(
                    JsonRpcId::Null,
                    error_codes::PARSE_ERROR,
                    "Parse error: invalid JSON",
                    None,
                ));
            }
        };

        let Some(obj) = parsed.as_object() else {
            return Err(boxed_error_response(
                JsonRpcId::Null,
                error_codes::INVALID_REQUEST,
                "Invalid Request: not a valid JSON-RPC 2.0 message",
                None,
            ));
        };
        if obj.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
            return Err(boxed_error_response(
                JsonRpcId::Null,
                error_codes::INVALID_REQUEST,
                "Invalid Request: not a valid JSON-RPC 2.0 message",
                None,
            ));
        }

        if obj.get("method").and_then(Value::as_str).is_none()
            && obj.contains_key("id")
            && (obj.contains_key("result") || obj.contains_key("error"))
        {
            // 有 id 且没有 method，但包含 result/error，是代理场景下从 daemon 返回的
            // response，不应误判成缺 method 的 request。
            let response: JsonRpcResponse = serde_json::from_value(parsed).map_err(|_| {
                boxed_error_response(
                    JsonRpcId::Null,
                    error_codes::INVALID_REQUEST,
                    "Invalid Request: not a valid JSON-RPC 2.0 response",
                    None,
                )
            })?;
            return Ok(Some(JsonRpcMessage::Response(response)));
        }

        if obj.get("method").and_then(Value::as_str).is_none() {
            return Err(boxed_error_response(
                JsonRpcId::Null,
                error_codes::INVALID_REQUEST,
                "Invalid Request: not a valid JSON-RPC 2.0 message",
                None,
            ));
        }

        if obj.contains_key("id") {
            Ok(Some(JsonRpcMessage::Request(
                serde_json::from_value(parsed).map_err(|_| {
                    boxed_error_response(
                        JsonRpcId::Null,
                        error_codes::INVALID_REQUEST,
                        "Invalid Request: not a valid JSON-RPC 2.0 request",
                        None,
                    )
                })?,
            )))
        } else {
            Ok(Some(JsonRpcMessage::Notification(
                serde_json::from_value(parsed).map_err(|_| {
                    boxed_error_response(
                        JsonRpcId::Null,
                        error_codes::INVALID_REQUEST,
                        "Invalid Request: not a valid JSON-RPC 2.0 notification",
                        None,
                    )
                })?,
            )))
        }
    }

    pub fn write_json(&mut self, value: &impl Serialize) {
        // 测试实现把写出的 NDJSON 留在内存里；真实 transport 复用同一序列化语义。
        if let Ok(mut line) = serde_json::to_string(value) {
            line.push('\n');
            self.sent_lines.push(line);
        }
    }
}

impl JsonRpcTransport for LineBasedJsonRpcTransport {
    fn stop(&mut self) {
        self.stopped = true;
        self.pending.clear();
    }

    fn send(&mut self, response: JsonRpcResponse) {
        self.write_json(&response);
    }

    fn notify(&mut self, method: &str, params: Option<Value>) {
        self.write_json(&JsonRpcNotification {
            jsonrpc: "2.0".to_string(),
            method: method.to_string(),
            params,
        });
    }

    fn send_result(&mut self, id: JsonRpcId, result: Value) {
        self.send(JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        });
    }

    fn send_error(&mut self, id: JsonRpcId, code: i64, message: &str, data: Option<Value>) {
        self.send(error_response(id, code, message, data));
    }
}

#[derive(Debug, Clone)]
pub struct StdioTransport {
    inner: LineBasedJsonRpcTransport,
    pub exit_on_close: bool,
}

impl Default for StdioTransport {
    fn default() -> Self {
        Self {
            inner: LineBasedJsonRpcTransport::new(),
            exit_on_close: true,
        }
    }
}

impl JsonRpcTransport for StdioTransport {
    fn stop(&mut self) {
        self.inner.stop();
    }

    fn send(&mut self, response: JsonRpcResponse) {
        self.inner.send(response);
    }

    fn notify(&mut self, method: &str, params: Option<Value>) {
        self.inner.notify(method, params);
    }

    fn send_result(&mut self, id: JsonRpcId, result: Value) {
        self.inner.send_result(id, result);
    }

    fn send_error(&mut self, id: JsonRpcId, code: i64, message: &str, data: Option<Value>) {
        self.inner.send_error(id, code, message, data);
    }
}

#[derive(Debug, Clone)]
pub struct SocketTransport {
    inner: LineBasedJsonRpcTransport,
    pub prefix: String,
    pub raw_lines: Vec<String>,
}

impl SocketTransport {
    pub fn new(prefix: impl Into<String>) -> Self {
        Self {
            inner: LineBasedJsonRpcTransport::new(),
            prefix: prefix.into(),
            raw_lines: Vec::new(),
        }
    }

    pub fn write_raw(&mut self, line: &str) {
        // proxy/daemon 握手有少量非 JSON 控制行，单独保留 raw_lines 以免污染
        // JSON-RPC sent_lines。
        let mut line = line.to_string();
        if !line.ends_with('\n') {
            line.push('\n');
        }
        self.raw_lines.push(line);
    }
}

impl JsonRpcTransport for SocketTransport {
    fn stop(&mut self) {
        self.inner.stop();
    }

    fn send(&mut self, response: JsonRpcResponse) {
        self.inner.send(response);
    }

    fn notify(&mut self, method: &str, params: Option<Value>) {
        self.inner.notify(method, params);
    }

    fn send_result(&mut self, id: JsonRpcId, result: Value) {
        self.inner.send_result(id, result);
    }

    fn send_error(&mut self, id: JsonRpcId, code: i64, message: &str, data: Option<Value>) {
        self.inner.send_error(id, code, message, data);
    }
}

pub fn error_response(
    id: JsonRpcId,
    code: i64,
    message: &str,
    data: Option<Value>,
) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id,
        result: None,
        error: Some(JsonRpcError {
            code,
            message: message.to_string(),
            data,
        }),
    }
}

fn boxed_error_response(
    id: JsonRpcId,
    code: i64,
    message: &str,
    data: Option<Value>,
) -> Box<JsonRpcResponse> {
    Box::new(error_response(id, code, message, data))
}
