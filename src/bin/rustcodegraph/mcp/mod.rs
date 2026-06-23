//! Rust CLI 二进制使用的小型 stdio MCP server。
//!
//! `session` 持有连接状态，`wire` 处理传输帧，`handler` 把 JSON-RPC 方法映射到
//! storage/query helper。

mod handler;
mod session;
mod wire;

pub(crate) use handler::handle_mcp_message;
pub(crate) use session::{
    McpStdioSession, current_parent_pid, install_mcp_ppid_watchdog, run_mcp_stdio,
};
pub(crate) use wire::{read_mcp_wire_message, write_mcp_wire_message};
