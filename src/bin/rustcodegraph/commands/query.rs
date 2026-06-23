//! 只读 CLI 查询命令。
//!
//! 这些命令面向人类终端使用，但尽量复用 MCP 的查询语义：符号搜索、节点源码、
//! caller/callee/impact 和 affected files 都走同一份 SQLite 索引数据。

use std::io::{self, Read};

use super::super::args::{
    command_path_arg, has_flag, option_value, path_option, positional_args, query_arg,
    resolve_project_path, result_limit,
};
use super::super::storage::{
    EdgeDirection, QueryMatch, affected_files_for_changes, edge_matches_for_symbol,
    find_symbol_nodes, is_test_file, normalize_index_path, open_sqlite_database, query_nodes,
    read_files, read_node_source, read_numbered_file_range,
};

pub(crate) fn command_files(args: &[String]) -> Result<(), String> {
    let project_root = resolve_project_path(path_option(args).or_else(|| command_path_arg(args)));
    let conn = open_sqlite_database(&project_root)?;
    let filter = option_value(args, "--filter").or_else(|| option_value(args, "-f"));
    let files = read_files(&conn, filter.as_deref())?;

    if has_flag(args, "-j", "--json") {
        println!(
            "{}",
            serde_json::to_string_pretty(&files).map_err(|err| err.to_string())?
        );
        return Ok(());
    }

    for file in &files {
        println!("{}", file.path);
    }
    Ok(())
}

pub(crate) fn command_query(args: &[String]) -> Result<(), String> {
    let Some(search) = query_arg(args) else {
        return Err("missing required <search> argument".to_owned());
    };
    let project_root = resolve_project_path(path_option(args));
    let conn = open_sqlite_database(&project_root)?;
    let limit = option_value(args, "-l")
        .or_else(|| option_value(args, "--limit"))
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(10);
    let kind_filter = option_value(args, "-k").or_else(|| option_value(args, "--kind"));
    let matches = query_nodes(&conn, &search, kind_filter.as_deref(), limit)?;

    if has_flag(args, "-j", "--json") {
        println!(
            "{}",
            serde_json::to_string_pretty(&matches).map_err(|err| err.to_string())?
        );
        return Ok(());
    }

    if matches.is_empty() {
        println!("No symbols found for \"{search}\"");
        return Ok(());
    }
    for node in &matches {
        println!(
            "{}  {}  {}:{}",
            node.kind, node.name, node.file_path, node.start_line
        );
    }
    Ok(())
}

pub(crate) fn command_node(args: &[String]) -> Result<(), String> {
    let project_root = resolve_project_path(path_option(args));
    if let Some(file) = option_value(args, "-f").or_else(|| option_value(args, "--file")) {
        // `node --file` 是调试索引内容的逃生口：绕过符号匹配，只按索引路径打印带行号源码。
        let offset = option_value(args, "--offset")
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(1);
        let limit = option_value(args, "--limit")
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(120);
        let text = read_numbered_file_range(&project_root, &file, offset, limit)?;
        println!("{text}");
        return Ok(());
    }

    let Some(symbol) = query_arg(args) else {
        return Err("missing required <name> argument".to_owned());
    };
    let conn = open_sqlite_database(&project_root)?;
    let matches = find_symbol_nodes(&conn, &symbol)?;
    if has_flag(args, "-j", "--json") {
        println!(
            "{}",
            serde_json::to_string_pretty(&matches).map_err(|err| err.to_string())?
        );
        return Ok(());
    }
    if matches.is_empty() {
        println!("No symbol found for \"{symbol}\"");
        return Ok(());
    }
    for node in matches.iter().take(result_limit(args, 5)) {
        println!(
            "{}  {}  {}:{}",
            node.kind, node.name, node.file_path, node.start_line
        );
        println!("{}", read_node_source(&project_root, node, 20)?);
    }
    Ok(())
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
    let conn = open_sqlite_database(&project_root)?;
    let limit = option_value(args, "--max-files")
        .or_else(|| option_value(args, "--limit"))
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(5);
    // CLI explore 是 MCP explore 的轻量近似：先做符号搜索，再把命中的源码片段直接展开。
    let matches = query_nodes(&conn, &terms, None, limit.max(1))?;
    if has_flag(args, "-j", "--json") {
        println!(
            "{}",
            serde_json::to_string_pretty(&matches).map_err(|err| err.to_string())?
        );
        return Ok(());
    }
    if matches.is_empty() {
        println!("No symbols found for \"{terms}\"");
        return Ok(());
    }
    println!("# RustCodeGraph Explore: {terms}");
    for node in &matches {
        println!();
        println!(
            "## {} `{}` at {}:{}",
            node.kind, node.name, node.file_path, node.start_line
        );
        println!("{}", read_node_source(&project_root, node, 30)?);
    }
    Ok(())
}

pub(crate) fn command_callers(args: &[String]) -> Result<(), String> {
    let Some(symbol) = query_arg(args) else {
        return Err("missing required <symbol> argument".to_owned());
    };
    let project_root = resolve_project_path(path_option(args));
    let conn = open_sqlite_database(&project_root)?;
    let limit = result_limit(args, 20);
    let matches = edge_matches_for_symbol(&conn, &symbol, EdgeDirection::Incoming, 1, limit)?;
    print_symbol_graph_matches(args, &matches, &symbol, "callers")
}

pub(crate) fn command_callees(args: &[String]) -> Result<(), String> {
    let Some(symbol) = query_arg(args) else {
        return Err("missing required <symbol> argument".to_owned());
    };
    let project_root = resolve_project_path(path_option(args));
    let conn = open_sqlite_database(&project_root)?;
    let limit = result_limit(args, 20);
    let matches = edge_matches_for_symbol(&conn, &symbol, EdgeDirection::Outgoing, 1, limit)?;
    print_symbol_graph_matches(args, &matches, &symbol, "callees")
}

pub(crate) fn command_impact(args: &[String]) -> Result<(), String> {
    let Some(symbol) = query_arg(args) else {
        return Err("missing required <symbol> argument".to_owned());
    };
    let project_root = resolve_project_path(path_option(args));
    let conn = open_sqlite_database(&project_root)?;
    let depth = option_value(args, "-d")
        .or_else(|| option_value(args, "--depth"))
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(2);
    let limit = result_limit(args, 50);
    let matches = edge_matches_for_symbol(&conn, &symbol, EdgeDirection::Incoming, depth, limit)?;
    print_symbol_graph_matches(args, &matches, &symbol, "impact")
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
