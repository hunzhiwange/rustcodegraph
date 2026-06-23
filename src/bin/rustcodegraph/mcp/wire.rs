//! MCP wire 帧读写辅助。
//!
//! 支持标准 `Content-Length` 帧，也支持测试和部分轻量 host 使用的 newline JSON。
//! 读取时记录原始 wire 格式，响应时用同一种格式写回，避免同一连接混用协议。

use std::io::{BufRead, Write};

use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum McpWireFormat {
    /// MCP stdio 使用的标准 Language Server Protocol 风格帧。
    ContentLength,
    /// 每行一个 JSON-RPC 消息，供测试和简单本地客户端使用。
    NewlineJson,
}

#[derive(Debug, Clone)]
pub(crate) struct McpWireMessage {
    pub(crate) value: Value,
    pub(crate) wire: McpWireFormat,
}

pub(crate) fn read_mcp_wire_message<R: BufRead>(
    reader: &mut R,
) -> Result<Option<McpWireMessage>, String> {
    // 跳过空行，兼容启动脚本或测试夹具多写的换行。
    let first_line = loop {
        let mut line = String::new();
        let bytes = reader
            .read_line(&mut line)
            .map_err(|err| format!("failed to read MCP message: {err}"))?;
        if bytes == 0 {
            return Ok(None);
        }
        if !line.trim().is_empty() {
            break line;
        }
    };

    let trimmed = first_line.trim_end_matches(['\r', '\n']);
    if !trimmed.to_ascii_lowercase().starts_with("content-length:") {
        // 非 header 首行按 newline JSON 处理；解析失败才返回协议错误。
        return serde_json::from_str(trimmed)
            .map(|value| {
                Some(McpWireMessage {
                    value,
                    wire: McpWireFormat::NewlineJson,
                })
            })
            .map_err(|err| format!("invalid newline JSON-RPC message: {err}"));
    }

    let mut content_length = trimmed
        .split_once(':')
        .and_then(|(_, value)| value.trim().parse::<usize>().ok());
    loop {
        // MCP/LSP header 到空行结束；若重复 Content-Length，后一个覆盖前一个。
        let mut line = String::new();
        let bytes = reader
            .read_line(&mut line)
            .map_err(|err| format!("failed to read MCP header: {err}"))?;
        if bytes == 0 {
            return Ok(None);
        }
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            break;
        }
        let lower = trimmed.to_ascii_lowercase();
        if let Some(value) = lower.strip_prefix("content-length:") {
            content_length = value.trim().parse::<usize>().ok();
        }
    }

    let Some(length) = content_length else {
        return Err("MCP message missing Content-Length header".to_owned());
    };
    let mut body = vec![0u8; length];
    reader
        .read_exact(&mut body)
        .map_err(|err| format!("failed to read MCP body: {err}"))?;
    serde_json::from_slice(&body)
        .map(|value| {
            Some(McpWireMessage {
                value,
                wire: McpWireFormat::ContentLength,
            })
        })
        .map_err(|err| format!("invalid MCP JSON: {err}"))
}

pub(crate) fn write_mcp_wire_message<W: Write>(
    writer: &mut W,
    response: &Value,
    wire: McpWireFormat,
) -> Result<(), String> {
    match wire {
        McpWireFormat::ContentLength => write_mcp_content_length_message(writer, response),
        McpWireFormat::NewlineJson => write_mcp_newline_message(writer, response),
    }
}

fn write_mcp_content_length_message<W: Write>(
    writer: &mut W,
    response: &Value,
) -> Result<(), String> {
    // Content-Length 必须按 UTF-8 字节数计算，不能用字符串字符数。
    let body = serde_json::to_vec(response).map_err(|err| err.to_string())?;
    write!(writer, "Content-Length: {}\r\n\r\n", body.len())
        .map_err(|err| format!("failed to write MCP header: {err}"))?;
    writer
        .write_all(&body)
        .map_err(|err| format!("failed to write MCP body: {err}"))?;
    writer
        .flush()
        .map_err(|err| format!("failed to flush MCP response: {err}"))
}

fn write_mcp_newline_message<W: Write>(writer: &mut W, response: &Value) -> Result<(), String> {
    serde_json::to_writer(&mut *writer, response)
        .map_err(|err| format!("failed to write JSON-RPC response: {err}"))?;
    writer
        .write_all(b"\n")
        .map_err(|err| format!("failed to write JSON-RPC newline: {err}"))?;
    writer
        .flush()
        .map_err(|err| format!("failed to flush JSON-RPC response: {err}"))
}
