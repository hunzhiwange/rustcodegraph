//! Rust CLI 分发器。
//!
//! 这里只做参数级分发和 help/version 的早返回；具体命令逻辑留在子模块，避免
//! 顶层入口同时承担索引、MCP、installer 和 release helper 的细节。

mod args;
mod commands;
mod daemon;
mod help;
mod indexer;
mod installer;
mod mcp;
mod release;
mod storage;

use commands::project::{
    command_index, command_init, command_status, command_sync, command_uninit, command_unlock,
    command_watch,
};
use commands::query::{
    command_affected, command_callees, command_callers, command_explore, command_files,
    command_impact, command_node, command_query,
};
use daemon::command_serve;
use help::{render_command_help, render_help};
use installer::{command_install, command_uninstall};
use release::{
    command_add_lang, command_extract_release_notes, command_prepare_release, command_upgrade,
    command_version,
};

pub fn run_cli(args: &[String]) -> Result<(), String> {
    // agent-eval/add-lang 有自己的子命令树；help 需要直接交给对应模块渲染。
    if args.first().map(String::as_str) == Some("agent-eval")
        && args.iter().any(|arg| arg == "-h" || arg == "--help")
    {
        return rustcodegraph::agent_eval::run_cli(args);
    }
    if args.first().map(String::as_str) == Some("add-lang")
        && args.iter().any(|arg| arg == "-h" || arg == "--help")
    {
        return rustcodegraph::add_lang::run_cli(args)
            .map(|output| print!("{}", output.text))
            .map_err(|err| err.message);
    }

    if args.is_empty() || args.iter().any(|arg| arg == "-h" || arg == "--help") {
        // `rustcodegraph query --help` 应展示 query 帮助，而不是顶层帮助。
        if let Some(command) = args.first().filter(|arg| !arg.starts_with('-'))
            && command != "help"
            && let Some(help) = render_command_help(command)
        {
            println!("{help}");
            return Ok(());
        }
        println!("{}", render_help());
        return Ok(());
    }

    if matches!(
        args.first().map(String::as_str),
        Some("-V" | "--version" | "-v" | "-version" | "version")
    ) {
        println!("{}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    if matches!(args.first().map(String::as_str), Some("help")) {
        if let Some(command) = args.get(1) {
            let Some(help) = render_command_help(command) else {
                return Err(format!("unknown command '{command}'"));
            };
            println!("{help}");
        } else {
            println!("{}", render_help());
        }
        return Ok(());
    }

    // 分发保持显式 match，新增命令时能同时更新 help spec 和这里的处理器。
    match args.first().map(String::as_str).unwrap_or_default() {
        "init" => command_init(args),
        "index" => command_index(args),
        "sync" => command_sync(args),
        "watch" => command_watch(args),
        "status" => command_status(args),
        "files" => command_files(args),
        "query" => command_query(args),
        "node" => command_node(args),
        "explore" => command_explore(args),
        "uninit" => command_uninit(args),
        "unlock" => command_unlock(args),
        "callers" => command_callers(args),
        "callees" => command_callees(args),
        "impact" => command_impact(args),
        "affected" => command_affected(args),
        "serve" => command_serve(args),
        "install" => command_install(args),
        "uninstall" => command_uninstall(args),
        "upgrade" => command_upgrade(args),
        "version" => command_version(args),
        "prepare-release" => command_prepare_release(args),
        "extract-release-notes" => command_extract_release_notes(args),
        "agent-eval" => rustcodegraph::agent_eval::run_cli(args),
        "add-lang" => command_add_lang(args),
        command => Err(format!("unknown command '{command}'")),
    }
}

pub fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if let Err(err) = run_cli(&args) {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}
