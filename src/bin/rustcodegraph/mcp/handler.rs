//! CLI stdio server 的 JSON-RPC/MCP 请求处理。
//!
//! 这个 handler 有意保持“失败尽量成功形态”：未索引、缺少 roots 等可恢复情况
//! 返回工具文本而不是 JSON-RPC 错误，避免宿主 agent 因早期错误放弃 rustcodegraph。

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

use rustcodegraph::mcp::server_instructions::{SERVER_INSTRUCTIONS, SERVER_INSTRUCTIONS_UNINDEXED};
use rustcodegraph::mcp::session::first_root_path;
use serde_json::{Value, json};

use super::session::McpStdioSession;
use super::wire::{McpWireFormat, read_mcp_wire_message, write_mcp_wire_message};
use crate::rustcodegraph_cli::args::resolve_project_path;
use crate::rustcodegraph_cli::storage::{
    EdgeDirection, edge_matches_for_symbol, find_symbol_nodes, format_matches, format_mcp_status,
    is_sqlite_initialized, open_sqlite_database, query_nodes, read_files, read_node_source,
    read_numbered_file_range, read_sqlite_stats,
};

pub(crate) fn handle_mcp_message<R: BufRead, W: Write>(
    session: &mut McpStdioSession,
    reader: &mut R,
    writer: &mut W,
    message: Value,
    wire: McpWireFormat,
) -> Result<(), String> {
    // Notification 没有 id，按 JSON-RPC 规范不需要响应；这包括 initialized 等消息。
    let id = message.get("id").cloned();
    let method = message.get("method").and_then(Value::as_str).unwrap_or("");
    let Some(id) = id else {
        return Ok(());
    };

    let response = match method {
        "initialize" => {
            session.capture_initialize_params(message.get("params"));
            let initialized = is_sqlite_initialized(&session.project_root);
            // 未索引时仍完成 initialize，但返回 inactive instructions；tools/list 再决定是否暴露工具。
            mcp_success(
                id,
                json!({
                    "protocolVersion": "2024-11-05",
                    "capabilities": { "tools": {} },
                    "serverInfo": {
                        "name": "rustcodegraph",
                        "version": env!("CARGO_PKG_VERSION"),
                    },
                    "instructions": if initialized { SERVER_INSTRUCTIONS } else { SERVER_INSTRUCTIONS_UNINDEXED },
                }),
            )
        }
        "tools/list" => {
            if !is_sqlite_initialized(&session.project_root) {
                maybe_resolve_project_from_client_roots(session, reader, writer, wire)?;
            }
            let initialized = is_sqlite_initialized(&session.project_root);
            // 空工具列表是最清晰的“当前项目未激活”信号，比暴露会失败的工具更不容易误导 agent。
            mcp_success(
                id,
                json!({ "tools": if initialized { mcp_tool_definitions() } else { Vec::<Value>::new() } }),
            )
        }
        "resources/list" => mcp_success(id, json!({ "resources": [] })),
        "prompts/list" => mcp_success(id, json!({ "prompts": [] })),
        "tools/call" => {
            let params = message.get("params").cloned().unwrap_or_else(|| json!({}));
            let name = params
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            let arguments = params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            if !has_mcp_project_path(&arguments) && !is_sqlite_initialized(&session.project_root) {
                maybe_resolve_project_from_client_roots(session, reader, writer, wire)?;
            }
            let tool_project_root = mcp_project_root(&session.project_root, &arguments);
            // 每个 projectRoot 只 catch up 一次，平衡首次准确性和每次工具调用的延迟。
            if should_catch_up_before_tool(&name) {
                session.catch_up_once(&tool_project_root);
            }
            let result = if !has_mcp_project_path(&arguments)
                && !is_sqlite_initialized(&session.project_root)
            {
                mcp_tool_result(not_indexed_mcp_message(&session.project_root), false)
            } else {
                match execute_mcp_tool(&session.project_root, &name, &arguments) {
                    Ok(text) => mcp_tool_result(text, false),
                    Err(err) => mcp_tool_result(err, true),
                }
            };
            mcp_success(id, result)
        }
        "ping" | "shutdown" => mcp_success(id, json!({})),
        _ => mcp_error(id, -32601, &format!("Method not found: {method}")),
    };

    write_mcp_wire_message(writer, &response, wire)?;
    // watcher 延迟到首次响应后启动，避免 initialize 本身被文件系统初始化成本拖慢。
    let watcher_started = if is_sqlite_initialized(&session.project_root) {
        session.start_watcher()
    } else {
        false
    };
    if method == "initialize" && is_sqlite_initialized(&session.project_root) {
        if watcher_started {
            eprintln!("[RustCodeGraph MCP] File watcher active - graph will auto-sync on changes");
        } else if !session.watch_enabled {
            eprintln!("[RustCodeGraph MCP] File watcher disabled by --no-watch");
        }
    }
    Ok(())
}

fn maybe_resolve_project_from_client_roots<R: BufRead, W: Write>(
    session: &mut McpStdioSession,
    reader: &mut R,
    writer: &mut W,
    wire: McpWireFormat,
) -> Result<(), String> {
    if session.roots_attempted || !session.client_supports_roots {
        return Ok(());
    }
    session.roots_attempted = true;

    // 有些 host 初始化时不给 rootUri，但支持 server-initiated roots/list；
    // 只尝试一次，避免在每次 tools/list/tools/call 上阻塞读循环。
    let request_id = session.next_roots_request_id();
    let request_id_value = Value::String(request_id.clone());
    write_mcp_wire_message(
        writer,
        &json!({
            "jsonrpc": "2.0",
            "id": request_id,
            "method": "roots/list",
        }),
        wire,
    )?;

    while let Some(message) = read_mcp_wire_message(reader)? {
        if message.value.get("id") != Some(&request_id_value) {
            // 这里丢弃其它消息而不是重入 dispatch，是为了保持实现简单；只在未索引冷启动路径触发。
            continue;
        }
        if let Some(root) = message.value.get("result").and_then(first_root_path) {
            session.project_root = resolve_project_path(Some(root.to_string_lossy().into_owned()));
        }
        return Ok(());
    }

    Ok(())
}

fn has_mcp_project_path(arguments: &Value) -> bool {
    mcp_string_arg(arguments, &["projectPath", "path"]).is_some()
}

fn not_indexed_mcp_message(searched: &Path) -> String {
    format!(
        "No RustCodeGraph project is loaded for this session.\nSearched for a .rustcodegraph/ directory starting from: {}\nIf this project is indexed, pass projectPath to the tool call or add --path to the MCP server config. If the project has no index, continue with built-in tools; indexing is the user's decision.",
        searched.display()
    )
}

fn mcp_success(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn mcp_error(id: Value, code: i64, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message }
    })
}

fn mcp_tool_result(text: String, is_error: bool) -> Value {
    json!({
        "content": [{ "type": "text", "text": text }],
        "isError": is_error
    })
}

fn mcp_tool_definitions() -> Vec<Value> {
    // 工具名与主 MCP server 对齐，但这里 schema 保持宽松，兼容不同 host 传入额外字段。
    vec![
        mcp_tool_definition(
            "rustcodegraph_query",
            "Search indexed symbols by name.",
            &[
                ("search", "Search text"),
                ("projectPath", "Optional project path"),
            ],
        ),
        mcp_tool_definition(
            "rustcodegraph_files",
            "List indexed files.",
            &[
                ("filter", "Optional path filter"),
                ("projectPath", "Optional project path"),
            ],
        ),
        mcp_tool_definition(
            "rustcodegraph_status",
            "Show index status and counts.",
            &[("projectPath", "Optional project path")],
        ),
        mcp_tool_definition(
            "rustcodegraph_node",
            "Show indexed source for a symbol or file.",
            &[
                ("name", "Symbol name"),
                ("file", "File path"),
                ("projectPath", "Optional project path"),
            ],
        ),
        mcp_tool_definition(
            "rustcodegraph_explore",
            "Return relevant symbols and source snippets for a query.",
            &[
                ("query", "Symbol/query text"),
                ("projectPath", "Optional project path"),
            ],
        ),
        mcp_tool_definition(
            "rustcodegraph_callers",
            "Find functions that call a symbol.",
            &[
                ("symbol", "Symbol name"),
                ("projectPath", "Optional project path"),
            ],
        ),
        mcp_tool_definition(
            "rustcodegraph_callees",
            "Find functions a symbol calls.",
            &[
                ("symbol", "Symbol name"),
                ("projectPath", "Optional project path"),
            ],
        ),
        mcp_tool_definition(
            "rustcodegraph_impact",
            "Find callers affected by changing a symbol.",
            &[
                ("symbol", "Symbol name"),
                ("depth", "Traversal depth"),
                ("projectPath", "Optional project path"),
            ],
        ),
    ]
}

fn mcp_tool_definition(name: &str, description: &str, props: &[(&str, &str)]) -> Value {
    let mut properties = serde_json::Map::new();
    for (name, description) in props {
        let value = if *name == "depth" || *name == "limit" {
            json!({ "type": "number", "description": description })
        } else {
            json!({ "type": "string", "description": description })
        };
        properties.insert((*name).to_owned(), value);
    }
    json!({
        "name": name,
        "description": description,
        "inputSchema": {
            "type": "object",
            "properties": properties,
            "additionalProperties": true
        }
    })
}

fn execute_mcp_tool(
    project_root: &Path,
    tool_name: &str,
    arguments: &Value,
) -> Result<String, String> {
    // 接受带/不带 rustcodegraph_ 前缀的名字，便于 CLI shim 与测试直接复用同一路径。
    let short = tool_name
        .strip_prefix("rustcodegraph_")
        .unwrap_or(tool_name);
    let project_root = mcp_project_root(project_root, arguments);
    let conn = open_sqlite_database(&project_root)?;
    let text = match short {
        "query" | "search" => {
            let search = mcp_string_arg(arguments, &["search", "query"])
                .ok_or_else(|| "missing search argument".to_owned())?;
            let limit = mcp_usize_arg(arguments, "limit").unwrap_or(10);
            format_matches(&query_nodes(&conn, &search, None, limit)?)
        }
        "files" => {
            let filter = mcp_string_arg(arguments, &["filter"]);
            let files = read_files(&conn, filter.as_deref())?;
            files
                .into_iter()
                .map(|file| file.path)
                .collect::<Vec<_>>()
                .join("\n")
        }
        "status" => {
            let stats = read_sqlite_stats(&conn)?;
            format_mcp_status(&stats)
        }
        "callers" => {
            let symbol = mcp_string_arg(arguments, &["symbol", "name"])
                .ok_or_else(|| "missing symbol argument".to_owned())?;
            format_matches(&edge_matches_for_symbol(
                &conn,
                &symbol,
                EdgeDirection::Incoming,
                1,
                mcp_usize_arg(arguments, "limit").unwrap_or(20),
            )?)
        }
        "callees" => {
            let symbol = mcp_string_arg(arguments, &["symbol", "name"])
                .ok_or_else(|| "missing symbol argument".to_owned())?;
            format_matches(&edge_matches_for_symbol(
                &conn,
                &symbol,
                EdgeDirection::Outgoing,
                1,
                mcp_usize_arg(arguments, "limit").unwrap_or(20),
            )?)
        }
        "impact" => {
            let symbol = mcp_string_arg(arguments, &["symbol", "name"])
                .ok_or_else(|| "missing symbol argument".to_owned())?;
            format_matches(&edge_matches_for_symbol(
                &conn,
                &symbol,
                EdgeDirection::Incoming,
                mcp_usize_arg(arguments, "depth").unwrap_or(2),
                mcp_usize_arg(arguments, "limit").unwrap_or(50),
            )?)
        }
        "node" => {
            if let Some(file) = mcp_string_arg(arguments, &["file", "path"]) {
                read_numbered_file_range(
                    &project_root,
                    &file,
                    mcp_usize_arg(arguments, "offset").unwrap_or(1),
                    mcp_usize_arg(arguments, "limit").unwrap_or(120),
                )?
            } else {
                let name = mcp_string_arg(arguments, &["name", "symbol"])
                    .ok_or_else(|| "missing name or file argument".to_owned())?;
                let matches = find_symbol_nodes(&conn, &name)?;
                let mut out = Vec::new();
                // 多个重名符号一次返回前几个候选的源码，减少 agent 为消歧再读文件的机会。
                for node in matches
                    .iter()
                    .take(mcp_usize_arg(arguments, "limit").unwrap_or(3))
                {
                    out.push(format!(
                        "{} `{}` at {}:{}\n{}",
                        node.kind,
                        node.name,
                        node.file_path,
                        node.start_line,
                        read_node_source(&project_root, node, 30)?
                    ));
                }
                out.join("\n\n")
            }
        }
        "explore" => {
            let query = mcp_string_arg(arguments, &["query", "search"])
                .ok_or_else(|| "missing query argument".to_owned())?;
            let matches = query_nodes(
                &conn,
                &query,
                None,
                mcp_usize_arg(arguments, "limit").unwrap_or(5),
            )?;
            let mut out = vec![format!("# RustCodeGraph Explore: {query}")];
            for node in &matches {
                out.push(format!(
                    "## {} `{}` at {}:{}\n{}",
                    node.kind,
                    node.name,
                    node.file_path,
                    node.start_line,
                    read_node_source(&project_root, node, 30)?
                ));
            }
            out.join("\n\n")
        }
        _ => return Err(format!("unknown RustCodeGraph tool: {tool_name}")),
    };
    Ok(with_mcp_index_state_notice(text, &project_root))
}

fn should_catch_up_before_tool(tool_name: &str) -> bool {
    let short = tool_name
        .strip_prefix("rustcodegraph_")
        .unwrap_or(tool_name);
    !matches!(short, "status")
}

fn with_mcp_index_state_notice(text: String, project_root: &Path) -> String {
    // 工具结果前置 stale/degraded 提醒，但保持 isError=false；这是“内容可能旧”的提示，
    // 不是协议或工具执行失败。
    let Ok(mut cg) = rustcodegraph::CodeGraph::open_sync(project_root) else {
        return text;
    };
    let degraded = cg.get_watcher_degraded_reason();
    let pending = cg.get_pending_files();
    cg.close();

    if let Some(reason) = degraded {
        return format!(
            "⚠️ RustCodeGraph auto-sync is DISABLED - live file watching stopped, so this index may be stale.\n  Reason: {reason}\n\n{text}"
        );
    }
    if pending.is_empty() {
        return text;
    }
    let files = pending
        .iter()
        .take(10)
        .map(|pending| format!("  - {}", pending.path))
        .collect::<Vec<_>>()
        .join("\n");
    let more = pending.len().saturating_sub(10);
    let suffix = if more > 0 {
        format!("\n  - ...and {more} more")
    } else {
        String::new()
    };
    format!(
        "⚠️ Some files were edited since the last index sync and may be stale in this response:\n{files}{suffix}\nThese files are waiting for the next batch sync; the watcher will refresh the graph automatically. Treat only those entries as possibly stale until that batch sync completes.\n\n{text}"
    )
}

#[cfg(test)]
mod tests {
    use super::should_catch_up_before_tool;

    #[test]
    fn status_is_diagnostic_and_does_not_trigger_catch_up() {
        assert!(!should_catch_up_before_tool("rustcodegraph_status"));
        assert!(!should_catch_up_before_tool("status"));
        assert!(should_catch_up_before_tool("rustcodegraph_search"));
        assert!(should_catch_up_before_tool("rustcodegraph_explore"));
    }
}

fn mcp_project_root(default_root: &Path, arguments: &Value) -> PathBuf {
    mcp_string_arg(arguments, &["projectPath", "path"])
        .map(|path| resolve_project_path(Some(path)))
        .unwrap_or_else(|| default_root.to_path_buf())
}

fn mcp_string_arg(arguments: &Value, names: &[&str]) -> Option<String> {
    names.iter().find_map(|name| {
        arguments
            .get(*name)
            .and_then(Value::as_str)
            .map(str::to_owned)
    })
}

fn mcp_usize_arg(arguments: &Value, name: &str) -> Option<usize> {
    arguments
        .get(name)
        .and_then(|value| {
            value
                .as_u64()
                .or_else(|| value.as_str()?.parse::<u64>().ok())
        })
        .map(|value| value as usize)
}
