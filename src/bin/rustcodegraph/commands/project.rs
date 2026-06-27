//! 会改变项目本地 RustCodeGraph 状态的 CLI 命令。
//!
//! 这里直接操作 SQLite 存储和 `.rustcodegraph/` 目录，保持 CLI 行为与库 facade 解耦；
//! MCP/watch 场景的更细粒度同步逻辑在库层维护。

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

use rustcodegraph::directory::{get_code_graph_dir, remove_directory};
use rustcodegraph::extraction::index::{hash_content, scan_directory};
use rustcodegraph::mcp::engine::parse_debounce_env;
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

    let started = Instant::now();
    let changes = detect_cli_sync_changes(&project_root)?;
    let mut changed_file_paths = Vec::new();
    changed_file_paths.extend(changes.added.iter().cloned());
    changed_file_paths.extend(changes.modified.iter().cloned());
    changed_file_paths.extend(changes.removed.iter().cloned());
    let files_checked = scan_directory(&project_root, None)
        .into_iter()
        .filter(|path| project_root.join(path).is_file())
        .count();
    let nodes_updated = if changed_file_paths.is_empty() {
        0
    } else {
        // Keep the CLI command on the same fast SQLite rebuild path as `index`.
        // The library facade sync does richer per-file extraction for MCP/watch
        // sessions, but a manual `rustcodegraph sync` must never wedge on one
        // native parser edge case when `rustcodegraph index` would finish.
        // 中文补充：CLI sync 宁可重建整库，也不在交互命令里暴露单文件 parser 的偶发失败。
        build_sqlite_index(&project_root, false)?.nodes_created
    };
    let result = rustcodegraph::SyncResult {
        files_checked,
        files_added: changes.added.len(),
        files_modified: changes.modified.len(),
        files_removed: changes.removed.len(),
        nodes_updated,
        duration_ms: started.elapsed().as_millis() as u64,
        changed_file_paths: (!changed_file_paths.is_empty()).then_some(changed_file_paths),
    };

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

    let debounce_ms = watch_debounce_ms(args)?;
    let mut graph = rustcodegraph::CodeGraph::open(
        &project_root,
        rustcodegraph::OpenOptions {
            sync: false,
            read_only: false,
        },
    )
    .map_err(|err| err.message().to_owned())?;

    let sync_result = graph.sync(rustcodegraph::IndexOptions::default());
    print_sync_summary(&sync_result);

    let started = graph.watch(rustcodegraph::WatchOptions { debounce_ms });
    graph.wait_until_watcher_ready(Some(10_000));
    if !started {
        let reason = graph
            .get_watcher_degraded_reason()
            .unwrap_or_else(|| "watcher backend failed to start".to_owned());
        graph.close();
        return Err(format!("failed to start file watcher: {reason}"));
    }

    let debounce_ms = debounce_ms.unwrap_or(2000);
    println!(
        "Watching {} for changes (debounce {}ms). Press Ctrl-C to stop.",
        project_root.display(),
        debounce_ms
    );

    let mut last_degraded_reason = None::<String>;
    let mut last_pending_paths = Vec::<String>::new();
    loop {
        let degraded_reason = graph.get_watcher_degraded_reason();
        if degraded_reason != last_degraded_reason {
            if let Some(reason) = &degraded_reason {
                eprintln!("warning: file watcher degraded: {reason}");
            }
            last_degraded_reason = degraded_reason;
        }
        let pending_paths = graph
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
                    "Detected {} pending file(s): {}{}",
                    pending_paths.len(),
                    preview,
                    suffix
                );
            }
            last_pending_paths = pending_paths;
        }
        thread::sleep(Duration::from_secs(1));
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

fn detect_cli_sync_changes(project_root: &Path) -> Result<rustcodegraph::ChangedFiles, String> {
    let conn = open_sqlite_database(project_root)?;
    let mut stmt = conn
        .prepare("SELECT path, content_hash FROM files")
        .map_err(|err| format!("failed to prepare file hash query: {err}"))?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|err| format!("failed to query file hashes: {err}"))?;
    let mut tracked = HashMap::new();
    for row in rows {
        let (path, hash) = row.map_err(|err| format!("failed to read file hash row: {err}"))?;
        tracked.insert(path, hash);
    }

    let current_files = scan_directory(project_root, None)
        .into_iter()
        .filter(|path| project_root.join(path).is_file())
        .collect::<HashSet<_>>();
    let mut changes = rustcodegraph::ChangedFiles::default();

    for path in tracked.keys() {
        if !current_files.contains(path) {
            changes.removed.push(path.clone());
        }
    }

    // 用文件内容 hash 做变更检测，避免只依赖 mtime 导致 checkout/restore 后漏同步。
    for path in current_files {
        let content = fs::read_to_string(project_root.join(&path))
            .map_err(|err| format!("failed to read {path}: {err}"))?;
        let current_hash = hash_content(&content);
        match tracked.get(&path) {
            None => changes.added.push(path),
            Some(previous_hash) if previous_hash != &current_hash => changes.modified.push(path),
            _ => {}
        }
    }

    changes.added.sort();
    changes.modified.sort();
    changes.removed.sort();
    Ok(changes)
}

fn watch_debounce_ms(args: &[String]) -> Result<Option<u64>, String> {
    if let Some(raw) = option_value(args, "--debounce-ms") {
        let Some(parsed) = parse_debounce_env(Some(raw.as_str())) else {
            return Err(format!(
                "invalid --debounce-ms value `{raw}`; expected an integer between 100 and 60000"
            ));
        };
        return Ok(Some(parsed));
    }
    Ok(parse_debounce_env(
        std::env::var("RUSTCODEGRAPH_WATCH_DEBOUNCE_MS")
            .ok()
            .as_deref(),
    ))
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
