//! 会改变项目本地 RustCodeGraph 状态的 CLI 命令。
//!
//! 这里直接操作 SQLite 存储和 `.rustcodegraph/` 目录；CLI 同步和 watch 自动刷新复用
//! 库 facade 的增量 sync，避免终端命令与 MCP/SDK 行为分叉。

use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use rustcodegraph::directory::{get_code_graph_dir, remove_directory};
use rustcodegraph::mcp::engine::{parse_debounce_env, parse_watch_policy_from_env};
use serde_json::json;

use super::super::args::{
    CLI_NAME, command_path_arg, guard_safe_root, has_flag, option_value, path_option,
    print_index_summary, print_sync_summary, resolve_init_path, resolve_project_path,
};
use super::super::indexer::build_sqlite_index;
use super::super::storage::{
    database_path, initialize_sqlite_database, is_sqlite_initialized, open_sqlite_database,
    read_last_indexed_at, read_sqlite_stats, unix_ms_to_iso,
};

pub(crate) fn command_init(args: &[String]) -> Result<(), String> {
    let project_root = resolve_init_path(command_path_arg(args));
    guard_safe_root(&project_root, args)?;
    let should_index = has_flag(args, "-i", "--index");
    let show_progress = !has_flag(args, "-q", "--quiet");

    if is_sqlite_initialized(&project_root) {
        println!(
            "RustCodeGraph already initialized in {}",
            project_root.display()
        );
        if should_index {
            let summary = build_sqlite_index(&project_root, show_progress)?;
            print_index_summary(&summary);
        } else {
            println!("Use \"{CLI_NAME} index\" to build or rebuild the index.");
        }
        return Ok(());
    }

    initialize_sqlite_database(&project_root, true)?;
    println!("Initialized RustCodeGraph in {}", project_root.display());

    if should_index {
        let summary = build_sqlite_index(&project_root, show_progress)?;
        print_index_summary(&summary);
    } else {
        println!(
            "Run \"{CLI_NAME} index\" to build the index, or use \"{CLI_NAME} init -i\" next time."
        );
    }
    Ok(())
}

pub(crate) fn command_index(args: &[String]) -> Result<(), String> {
    let project_root = resolve_project_path(command_path_arg(args));
    guard_safe_root(&project_root, args)?;
    let show_progress = !has_flag(args, "-q", "--quiet");

    if !is_sqlite_initialized(&project_root) {
        return Err(format!(
            "RustCodeGraph not initialized in {}. Run \"{CLI_NAME} init -i\" first.",
            project_root.display()
        ));
    }

    let summary = build_sqlite_index(&project_root, show_progress)?;
    if !has_flag(args, "-q", "--quiet") {
        print_index_summary(&summary);
    }
    Ok(())
}

pub(crate) fn command_sync(args: &[String]) -> Result<(), String> {
    let project_root = resolve_project_path(command_path_arg(args));
    guard_safe_root(&project_root, args)?;
    let quiet = has_flag(args, "-q", "--quiet");

    if !is_sqlite_initialized(&project_root) {
        return Err(format!(
            "RustCodeGraph not initialized in {}. Run \"{CLI_NAME} init -i\" first.",
            project_root.display()
        ));
    }

    let result = run_cli_sync(&project_root)?;

    if !quiet {
        print_sync_summary(&result);
    }
    Ok(())
}

pub(crate) fn command_watch(args: &[String]) -> Result<(), String> {
    let project_root = resolve_project_path(path_option(args).or_else(|| command_path_arg(args)));
    guard_safe_root(&project_root, args)?;

    if !is_sqlite_initialized(&project_root) {
        return Err(format!(
            "RustCodeGraph not initialized in {}. Run \"{CLI_NAME} init -i\" first.",
            project_root.display()
        ));
    }

    let watch_options = watch_options(args)?;
    let timing = resolved_watch_timing(&watch_options);

    let sync_result = run_cli_sync(&project_root)?;
    rustcodegraph::debug_rss_pub("cmd:after startup sync");
    print_sync_summary(&sync_result);

    let latest_auto_sync = Arc::new(Mutex::new(None::<rustcodegraph::SyncResult>));
    let sync_project_root = project_root.clone();
    let sync_latest_auto_sync = Arc::clone(&latest_auto_sync);
    let print_latest_auto_sync = Arc::clone(&latest_auto_sync);
    let mut watcher = rustcodegraph::sync::watcher::FileWatcher::new(
        &project_root,
        move || {
            let result = run_cli_sync(&sync_project_root)
                .map_err(rustcodegraph::sync::watcher::WatchSyncError::Other)?;
            let files_changed = result.files_added + result.files_modified + result.files_removed;
            if let Ok(mut slot) = sync_latest_auto_sync.lock() {
                *slot = Some(result.clone());
            }
            Ok(rustcodegraph::sync::watcher::SyncRunResult {
                files_changed,
                duration_ms: result.duration_ms,
                skipped: false,
            })
        },
        rustcodegraph::sync::watcher::WatchOptions {
            debounce_ms: Some(timing.debounce_ms),
            max_debounce_ms: Some(timing.max_debounce_ms),
            min_sync_interval_ms: Some(timing.min_sync_interval_ms),
            on_sync_complete: Some(Box::new(move |result| {
                if let Ok(mut slot) = print_latest_auto_sync.lock() {
                    if let Some(sync_result) = slot.take() {
                        print_sync_summary(&sync_result);
                        return;
                    }
                }
                println!(
                    "Synced {} changed file(s) in {}ms",
                    result.files_changed, result.duration_ms
                );
            })),
            ..rustcodegraph::sync::watcher::WatchOptions::default()
        },
    );
    let started = watcher.start();
    rustcodegraph::debug_rss_pub("cmd:after watcher.start()");
    if started {
        watcher
            .wait_until_ready(10_000)
            .map_err(|err| format!("failed to start file watcher: {err}"))?;
    }
    rustcodegraph::debug_rss_pub("cmd:after wait_until_watcher_ready");
    if !started {
        let reason = watcher
            .get_degraded_reason()
            .map(str::to_owned)
            .unwrap_or_else(|| "watcher backend failed to start".to_owned());
        return Err(format!("failed to start file watcher: {reason}"));
    }

    println!(
        "Watching {} for changes (debounce {}ms, max wait {}ms, min interval {}ms). Press Ctrl-C to stop.",
        project_root.display(),
        timing.debounce_ms,
        timing.max_debounce_ms,
        timing.min_sync_interval_ms
    );

    let mut last_degraded_reason = None::<String>;
    let mut last_pending_paths = Vec::<String>::new();
    let poll_interval = Duration::from_millis(timing.debounce_ms.clamp(50, 250));
    loop {
        watcher.tick();
        let degraded_reason = watcher.get_degraded_reason().map(str::to_owned);
        if degraded_reason != last_degraded_reason {
            if let Some(reason) = &degraded_reason {
                eprintln!("warning: file watcher degraded: {reason}");
            }
            last_degraded_reason = degraded_reason;
        }
        let pending_paths = watcher
            .get_pending_files()
            .into_iter()
            .map(|pending| pending.path)
            .collect::<Vec<_>>();
        if pending_paths != last_pending_paths {
            if pending_paths.is_empty() && !last_pending_paths.is_empty() {
                println!("Auto-sync caught up.");
            } else if !pending_paths.is_empty() {
                let preview = pending_paths
                    .iter()
                    .take(3)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ");
                let suffix = if pending_paths.len() > 3 { ", ..." } else { "" };
                println!(
                    "Detected {} pending file(s) waiting for the next batch sync (debounce {}ms, max wait {}ms, min interval {}ms): {}{}",
                    pending_paths.len(),
                    timing.debounce_ms,
                    timing.max_debounce_ms,
                    timing.min_sync_interval_ms,
                    preview,
                    suffix
                );
            }
            last_pending_paths = pending_paths;
        }
        thread::sleep(poll_interval);
    }
}

fn run_cli_sync(project_root: &Path) -> Result<rustcodegraph::SyncResult, String> {
    let mut graph = rustcodegraph::CodeGraph::open(
        project_root,
        rustcodegraph::OpenOptions {
            sync: false,
            read_only: false,
        },
    )
    .map_err(|err| err.message().to_owned())?;
    let result = graph.sync(rustcodegraph::IndexOptions::default());
    graph.close();
    Ok(result)
}

#[derive(Debug, Clone, Copy)]
struct WatchTiming {
    debounce_ms: u64,
    max_debounce_ms: u64,
    min_sync_interval_ms: u64,
}

fn resolved_watch_timing(options: &rustcodegraph::WatchOptions) -> WatchTiming {
    const DEFAULT_DEBOUNCE_MS: u64 = 2000;
    const DEFAULT_MAX_DEBOUNCE_MULTIPLIER: u64 = 5;
    const MAX_WATCH_WINDOW_MS: u64 = 60_000;

    let debounce_ms = options.debounce_ms.unwrap_or(DEFAULT_DEBOUNCE_MS);
    let max_debounce_ms = options
        .max_debounce_ms
        .unwrap_or_else(|| debounce_ms.saturating_mul(DEFAULT_MAX_DEBOUNCE_MULTIPLIER))
        .max(debounce_ms)
        .min(MAX_WATCH_WINDOW_MS);
    let min_sync_interval_ms = options
        .min_sync_interval_ms
        .unwrap_or(debounce_ms)
        .min(MAX_WATCH_WINDOW_MS);

    WatchTiming {
        debounce_ms,
        max_debounce_ms,
        min_sync_interval_ms,
    }
}

pub(crate) fn command_uninit(args: &[String]) -> Result<(), String> {
    let project_root = resolve_project_path(command_path_arg(args));
    if !has_flag(args, "-f", "--force") {
        return Err("uninit requires -f/--force in the Rust port".to_owned());
    }
    let hooks = rustcodegraph::sync::git_hooks::remove_git_sync_hook(&project_root, None);
    remove_directory(&project_root).map_err(|err| err.to_string())?;
    println!("Removed RustCodeGraph data from {}", project_root.display());
    if !hooks.installed.is_empty() {
        let names = hooks
            .installed
            .iter()
            .map(|hook| hook.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        println!("Removed git sync hook(s): {names}");
    }
    Ok(())
}

pub(crate) fn command_unlock(args: &[String]) -> Result<(), String> {
    let project_root = resolve_project_path(command_path_arg(args));
    let lock_path = get_code_graph_dir(&project_root).join("rustcodegraph.lock");
    if !lock_path.exists() {
        println!("No lock file found at {}", lock_path.display());
        return Ok(());
    }
    fs::remove_file(&lock_path)
        .map_err(|err| format!("failed to remove {}: {err}", lock_path.display()))?;
    println!("Removed stale lock file {}", lock_path.display());
    Ok(())
}

fn watch_options(args: &[String]) -> Result<rustcodegraph::WatchOptions, String> {
    let mut options = parse_watch_policy_from_env();
    if let Some(raw) = option_value(args, "--debounce-ms") {
        let Some(parsed) = parse_debounce_env(Some(raw.as_str())) else {
            return Err(format!(
                "invalid --debounce-ms value `{raw}`; expected an integer between 100 and 60000"
            ));
        };
        options.debounce_ms = Some(parsed);
    }
    Ok(options)
}

pub(crate) fn command_status(args: &[String]) -> Result<(), String> {
    let project_root = resolve_project_path(command_path_arg(args));
    let json_output = has_flag(args, "-j", "--json");
    if !is_sqlite_initialized(&project_root) {
        if json_output {
            // JSON 模式即使未初始化也返回机器可读状态，方便安装器和脚本做探测。
            println!(
                "{}",
                serde_json::to_string(&json!({
                    "initialized": false,
                    "version": env!("CARGO_PKG_VERSION"),
                    "projectPath": project_root.to_string_lossy(),
                    "indexPath": get_code_graph_dir(&project_root).to_string_lossy(),
                    "lastIndexed": Option::<String>::None,
                }))
                .map_err(|err| err.to_string())?
            );
            return Ok(());
        }
        return Err(format!(
            "RustCodeGraph not initialized in {}. Run \"{CLI_NAME} init -i\" first.",
            project_root.display()
        ));
    }

    let conn = open_sqlite_database(&project_root)?;
    let stats = read_sqlite_stats(&conn)?;
    if json_output {
        let last_indexed = read_last_indexed_at(&conn)?;
        println!(
            "{}",
            serde_json::to_string(&json!({
                "initialized": true,
                "version": env!("CARGO_PKG_VERSION"),
                "projectPath": project_root.to_string_lossy(),
                "indexPath": get_code_graph_dir(&project_root).to_string_lossy(),
                "lastIndexed": last_indexed.map(unix_ms_to_iso),
                "fileCount": stats.file_count,
                "nodeCount": stats.node_count,
                "edgeCount": stats.edge_count,
                "backend": "sqlite",
                "nodesByKind": stats.nodes_by_kind,
                "filesByLanguage": stats.files_by_language,
            }))
            .map_err(|err| err.to_string())?
        );
        return Ok(());
    }

    println!("RustCodeGraph status");
    println!("  Project: {}", project_root.display());
    println!("  Backend: sqlite");
    println!("  Files: {}", stats.file_count);
    println!("  Nodes: {}", stats.node_count);
    println!("  Edges: {}", stats.edge_count);
    println!("  Indexed at: {}", stats.last_updated);
    println!("  Database: {}", database_path(&project_root).display());
    if !stats.files_by_language.is_empty() {
        println!();
        println!("Files by language:");
        for (language, count) in stats.files_by_language {
            println!("  {language}: {count}");
        }
    }
    if !stats.nodes_by_kind.is_empty() {
        println!();
        println!("Nodes by kind:");
        for (kind, count) in stats.nodes_by_kind {
            println!("  {kind}: {count}");
        }
    }
    Ok(())
}
