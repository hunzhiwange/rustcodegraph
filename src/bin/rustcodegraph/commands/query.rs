//! 只读 CLI 查询命令。
//!
//! 这些命令面向人类终端使用，但尽量复用 MCP 的查询语义：符号搜索、节点源码、
//! caller/callee/impact 和 affected files 都走同一份 SQLite 索引数据。

use std::io::{self, Read};
use std::path::Path;

use rustcodegraph::mcp::tools::{ToolHandler, ToolResult};
use serde_json::{Map, Value, json};

use super::super::args::{
    command_path_arg, has_flag, option_value, path_option, positional_args, query_arg,
    resolve_project_path, result_limit,
};
use super::super::storage::{
    EdgeDirection, QueryMatch, affected_files_for_changes, edge_matches_for_symbol,
    find_symbol_nodes, is_test_file, normalize_index_path, open_sqlite_database, query_nodes,
    read_files,
};

pub(crate) fn command_files(args: &[String]) -> Result<(), String> {
    let project_root = resolve_project_path(path_option(args).or_else(|| command_path_arg(args)));
    let filter = option_value(args, "--filter").or_else(|| option_value(args, "-f"));

    if has_flag(args, "-j", "--json") {
        let conn = open_sqlite_database(&project_root)?;
        let files = read_files(&conn, filter.as_deref())?;
        println!(
            "{}",
            serde_json::to_string_pretty(&files).map_err(|err| err.to_string())?
        );
        return Ok(());
    }

    let mut tool_args = Map::new();
    if let Some(filter) = filter.or_else(|| command_path_arg(args)) {
        tool_args.insert("path".to_string(), json!(filter));
    }
    if let Some(pattern) = option_value(args, "--pattern") {
        tool_args.insert("pattern".to_string(), json!(pattern));
    }
    if let Some(format) = option_value(args, "--format") {
        tool_args.insert("format".to_string(), json!(format));
    }
    if has_flag(args, "--no-metadata", "--no-metadata") {
        tool_args.insert("includeMetadata".to_string(), json!(false));
    }
    insert_u64_arg(
        &mut tool_args,
        "maxDepth",
        option_value(args, "--max-depth"),
    );
    print_mcp_tool_text("rustcodegraph_files", &project_root, tool_args)
}

pub(crate) fn command_query(args: &[String]) -> Result<(), String> {
    let Some(search) = query_arg(args) else {
        return Err("missing required <search> argument".to_owned());
    };
    let project_root = resolve_project_path(path_option(args));
    let limit = option_value(args, "-l")
        .or_else(|| option_value(args, "--limit"))
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(10);
    let kind_filter = option_value(args, "-k").or_else(|| option_value(args, "--kind"));

    if has_flag(args, "-j", "--json") {
        let conn = open_sqlite_database(&project_root)?;
        let matches = query_nodes(&conn, &search, kind_filter.as_deref(), limit)?;
        println!(
            "{}",
            serde_json::to_string_pretty(&matches).map_err(|err| err.to_string())?
        );
        return Ok(());
    }

    let mut tool_args = Map::new();
    tool_args.insert("query".to_string(), json!(search));
    tool_args.insert("limit".to_string(), json!(limit));
    if let Some(kind) = kind_filter {
        tool_args.insert("kind".to_string(), json!(kind));
    }
    print_mcp_tool_text("rustcodegraph_search", &project_root, tool_args)
}

pub(crate) fn command_node(args: &[String]) -> Result<(), String> {
    let project_root = resolve_project_path(path_option(args));
    let file_hint = option_value(args, "-f").or_else(|| option_value(args, "--file"));
    let symbol = query_arg(args).unwrap_or_default();
    if symbol.trim().is_empty()
        && let Some(file) = file_hint.as_deref()
    {
        let mut tool_args = Map::new();
        tool_args.insert("file".to_string(), json!(file));
        if has_flag(args, "--symbols-only", "--symbols-only") {
            tool_args.insert("symbolsOnly".to_string(), json!(true));
        }
        insert_u64_arg(&mut tool_args, "offset", option_value(args, "--offset"));
        insert_u64_arg(&mut tool_args, "limit", option_value(args, "--limit"));
        if has_flag(args, "-j", "--json") {
            let result = execute_mcp_tool("rustcodegraph_node", &project_root, tool_args);
            println!(
                "{}",
                serde_json::to_string_pretty(&result).map_err(|err| err.to_string())?
            );
            return Ok(());
        }
        return print_mcp_tool_text("rustcodegraph_node", &project_root, tool_args);
    }

    if symbol.trim().is_empty() {
        return Err("missing required <name> argument".to_owned());
    }
    if has_flag(args, "-j", "--json") {
        let conn = open_sqlite_database(&project_root)?;
        let matches = find_symbol_nodes(&conn, &symbol)?;
        println!(
            "{}",
            serde_json::to_string_pretty(&matches).map_err(|err| err.to_string())?
        );
        return Ok(());
    }

    let mut tool_args = Map::new();
    tool_args.insert("symbol".to_string(), json!(symbol));
    tool_args.insert("includeCode".to_string(), json!(true));
    if let Some(file) = file_hint {
        tool_args.insert("file".to_string(), json!(file));
    }
    if has_flag(args, "--symbols-only", "--symbols-only") {
        tool_args.insert("symbolsOnly".to_string(), json!(true));
    }
    insert_u64_arg(&mut tool_args, "offset", option_value(args, "--offset"));
    insert_u64_arg(&mut tool_args, "limit", option_value(args, "--limit"));
    print_mcp_tool_text("rustcodegraph_node", &project_root, tool_args)
}

pub(crate) fn command_explore(args: &[String]) -> Result<(), String> {
    let terms = positional_args(args)
        .into_iter()
        .skip(1)
        .collect::<Vec<_>>()
        .join(" ");
    if terms.trim().is_empty() {
        return Err("missing required <query> argument".to_owned());
    }
    let project_root = resolve_project_path(path_option(args));
    let limit = option_value(args, "--max-files")
        .or_else(|| option_value(args, "--limit"))
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(5);
    if has_flag(args, "-j", "--json") {
        let result = execute_mcp_tool(
            "rustcodegraph_explore",
            &project_root,
            mcp_explore_args(&terms, limit),
        );
        println!(
            "{}",
            serde_json::to_string_pretty(&result).map_err(|err| err.to_string())?
        );
        return Ok(());
    }
    print_mcp_tool_text(
        "rustcodegraph_explore",
        &project_root,
        mcp_explore_args(&terms, limit),
    )
}

pub(crate) fn command_callers(args: &[String]) -> Result<(), String> {
    let Some(symbol) = query_arg(args) else {
        return Err("missing required <symbol> argument".to_owned());
    };
    let project_root = resolve_project_path(path_option(args));
    let limit = result_limit(args, 20);
    if has_flag(args, "-j", "--json") {
        let conn = open_sqlite_database(&project_root)?;
        let matches = edge_matches_for_symbol(&conn, &symbol, EdgeDirection::Incoming, 1, limit)?;
        return print_symbol_graph_matches(args, &matches, &symbol, "callers");
    }
    print_mcp_tool_text(
        "rustcodegraph_callers",
        &project_root,
        mcp_symbol_args(args, &symbol, limit, None),
    )
}

pub(crate) fn command_callees(args: &[String]) -> Result<(), String> {
    let Some(symbol) = query_arg(args) else {
        return Err("missing required <symbol> argument".to_owned());
    };
    let project_root = resolve_project_path(path_option(args));
    let limit = result_limit(args, 20);
    if has_flag(args, "-j", "--json") {
        let conn = open_sqlite_database(&project_root)?;
        let matches = edge_matches_for_symbol(&conn, &symbol, EdgeDirection::Outgoing, 1, limit)?;
        return print_symbol_graph_matches(args, &matches, &symbol, "callees");
    }
    print_mcp_tool_text(
        "rustcodegraph_callees",
        &project_root,
        mcp_symbol_args(args, &symbol, limit, None),
    )
}

pub(crate) fn command_impact(args: &[String]) -> Result<(), String> {
    let Some(symbol) = query_arg(args) else {
        return Err("missing required <symbol> argument".to_owned());
    };
    let project_root = resolve_project_path(path_option(args));
    let depth = option_value(args, "-d")
        .or_else(|| option_value(args, "--depth"))
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(2);
    let limit = result_limit(args, 50);
    if has_flag(args, "-j", "--json") {
        let conn = open_sqlite_database(&project_root)?;
        let matches =
            edge_matches_for_symbol(&conn, &symbol, EdgeDirection::Incoming, depth, limit)?;
        return print_symbol_graph_matches(args, &matches, &symbol, "impact");
    }
    print_mcp_tool_text(
        "rustcodegraph_impact",
        &project_root,
        mcp_symbol_args(args, &symbol, limit, Some(depth)),
    )
}

pub(crate) fn command_affected(args: &[String]) -> Result<(), String> {
    let project_root = resolve_project_path(path_option(args));
    let conn = open_sqlite_database(&project_root)?;
    let mut files = positional_args(args)
        .into_iter()
        .skip(1)
        .collect::<Vec<_>>();
    if has_flag(args, "--stdin", "--stdin") {
        let mut input = String::new();
        io::stdin()
            .read_to_string(&mut input)
            .map_err(|err| format!("failed to read stdin: {err}"))?;
        files.extend(
            input
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(str::to_owned),
        );
    }
    files = files
        .into_iter()
        .map(|file| normalize_index_path(&file, &project_root))
        .filter(|file| !file.is_empty())
        .collect();
    if files.is_empty() {
        return Err("missing changed files; pass paths or --stdin".to_owned());
    }
    let depth = option_value(args, "-d")
        .or_else(|| option_value(args, "--depth"))
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(5);
    let affected = affected_files_for_changes(&conn, &files, depth)?;
    let test_files = affected
        .iter()
        .filter(|path| is_test_file(path))
        .cloned()
        .collect::<Vec<_>>();
    // 默认优先输出受影响测试文件；没有测试命中时才回退到全部受影响文件，便于 CI 直接消费。
    let output = if test_files.is_empty() {
        affected
    } else {
        test_files
    };
    if has_flag(args, "-j", "--json") {
        println!(
            "{}",
            serde_json::to_string_pretty(&output).map_err(|err| err.to_string())?
        );
        return Ok(());
    }
    if has_flag(args, "-q", "--quiet") {
        for path in output {
            println!("{path}");
        }
        return Ok(());
    }
    if output.is_empty() {
        println!("No affected indexed files found");
    } else {
        println!("Affected files:");
        for path in output {
            println!("  {path}");
        }
    }
    Ok(())
}

fn print_symbol_graph_matches(
    args: &[String],
    matches: &[QueryMatch],
    symbol: &str,
    label: &str,
) -> Result<(), String> {
    if has_flag(args, "-j", "--json") {
        // 图查询的 JSON 输出保持原始 QueryMatch，避免文本格式变化影响脚本用户。
        println!(
            "{}",
            serde_json::to_string_pretty(matches).map_err(|err| err.to_string())?
        );
        return Ok(());
    }

    if matches.is_empty() {
        println!("No {label} found for \"{symbol}\"");
        return Ok(());
    }

    for node in matches {
        println!(
            "{}  {}  {}:{}",
            node.kind, node.name, node.file_path, node.start_line
        );
    }
    Ok(())
}

fn mcp_explore_args(query: &str, max_files: usize) -> Map<String, Value> {
    let mut args = Map::new();
    args.insert("query".to_string(), json!(query));
    args.insert("maxFiles".to_string(), json!(max_files.max(1)));
    args
}

fn mcp_symbol_args(
    cli_args: &[String],
    symbol: &str,
    limit: usize,
    depth: Option<usize>,
) -> Map<String, Value> {
    let mut args = Map::new();
    args.insert("symbol".to_string(), json!(symbol));
    args.insert("limit".to_string(), json!(limit.max(1)));
    if let Some(file) = option_value(cli_args, "--file").or_else(|| option_value(cli_args, "-f")) {
        args.insert("file".to_string(), json!(file));
    }
    if let Some(depth) = depth {
        args.insert("depth".to_string(), json!(depth.max(1)));
    }
    args
}

fn insert_u64_arg(args: &mut Map<String, Value>, name: &str, raw: Option<String>) {
    if let Some(value) = raw.and_then(|value| value.parse::<u64>().ok()) {
        args.insert(name.to_string(), json!(value));
    }
}

fn print_mcp_tool_text(
    tool_name: &str,
    project_root: &Path,
    args: Map<String, Value>,
) -> Result<(), String> {
    let result = execute_mcp_tool(tool_name, project_root, args);
    let text = tool_result_text(&result);
    if result.is_error == Some(true) {
        return Err(text
            .strip_prefix("Error: ")
            .unwrap_or(text.as_str())
            .to_string());
    }
    println!("{text}");
    Ok(())
}

fn execute_mcp_tool(tool_name: &str, project_root: &Path, args: Map<String, Value>) -> ToolResult {
    let root = project_root.to_string_lossy().into_owned();
    let mut handler = ToolHandler::new(true);
    handler.set_default_project_hint(root.clone());
    handler.set_default_project_root(root);
    let result = handler.execute_for_cli(tool_name, &args);
    handler.close_all();
    result
}

fn tool_result_text(result: &ToolResult) -> String {
    result
        .content
        .iter()
        .filter(|content| content.content_type == "text")
        .map(|content| content.text.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}
