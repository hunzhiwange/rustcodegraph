//! CLI 参数与路径解析 helpers。
//!
//! 这里仍是轻量手写解析：installer/npm 启动路径需要保持依赖少、启动快，
//! 所以只集中维护 rustcodegraph 子命令实际用到的选项形状。

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rustcodegraph::directory::unsafe_index_root_reason;
use rustcodegraph::types::TimestampMs;

use super::indexer::IndexSummary;
use super::storage::is_sqlite_initialized;

pub(crate) const CLI_NAME: &str = "rustcodegraph";

pub(crate) fn print_index_summary(summary: &IndexSummary) {
    println!(
        "Indexed {} files, skipped {}, created {} nodes and {} edges in {}ms",
        summary.files_indexed,
        summary.files_skipped,
        summary.nodes_created,
        summary.edges_created,
        summary.duration_ms
    );
}

pub(crate) fn print_sync_summary(result: &rustcodegraph::SyncResult) {
    let changed = result.files_added + result.files_modified + result.files_removed;
    println!(
        "Synced {} changed file(s): {} added, {} modified, {} removed in {}ms",
        changed,
        result.files_added,
        result.files_modified,
        result.files_removed,
        result.duration_ms
    );
}

pub(crate) fn guard_safe_root(project_root: &Path, args: &[String]) -> Result<(), String> {
    if has_flag(args, "-f", "--force") {
        return Ok(());
    }
    // 防止误把 home、磁盘根等大目录索引进 .rustcodegraph；强制模式只给显式用户操作。
    if let Some(reason) = unsafe_index_root_reason(project_root) {
        return Err(format!(
            "Refusing to index {} because it looks like {reason}. Pass --force to override.",
            project_root.display()
        ));
    }
    Ok(())
}

pub(crate) fn command_path_arg(args: &[String]) -> Option<String> {
    positional_args(args).into_iter().nth(1)
}

pub(crate) fn query_arg(args: &[String]) -> Option<String> {
    positional_args(args).into_iter().nth(1)
}

pub(crate) fn result_limit(args: &[String], default_limit: usize) -> usize {
    option_value(args, "-l")
        .or_else(|| option_value(args, "--limit"))
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(default_limit)
}

pub(crate) fn positional_args(args: &[String]) -> Vec<String> {
    let mut values = Vec::new();
    let mut iter = args.iter().peekable();
    while let Some(arg) = iter.next() {
        // 手写解析时必须跳过“吃值”的 option，否则 `--path foo` 里的 foo 会被误当成命令参数。
        if option_takes_value(arg) {
            let _ = iter.next();
            continue;
        }
        if arg.starts_with('-') {
            continue;
        }
        values.push(arg.clone());
    }
    values
}

pub(crate) fn path_option(args: &[String]) -> Option<String> {
    option_value(args, "-p").or_else(|| option_value(args, "--path"))
}

pub(crate) fn option_value(args: &[String], name: &str) -> Option<String> {
    for idx in 0..args.len() {
        let arg = &args[idx];
        if arg == name {
            return args.get(idx + 1).cloned();
        }
        if let Some(value) = arg.strip_prefix(&format!("{name}=")) {
            return Some(value.to_owned());
        }
    }
    None
}

fn option_takes_value(arg: &str) -> bool {
    // 这个列表是 positional_args 的安全网；新增带值 option 时要同步加到这里。
    matches!(
        arg,
        "-p" | "--path"
            | "-l"
            | "--limit"
            | "-k"
            | "--kind"
            | "-d"
            | "--depth"
            | "--filter"
            | "-t"
            | "--target"
            | "--file"
            | "--format"
            | "--max-files"
            | "--max-depth"
            | "--offset"
            | "--print-config"
            | "--location"
    )
}

pub(crate) fn has_flag(args: &[String], short: &str, long: &str) -> bool {
    args.iter().any(|arg| arg == short || arg == long)
}

pub(crate) fn resolve_init_path(path_arg: Option<String>) -> PathBuf {
    absolutize(path_arg.unwrap_or_else(|| ".".to_owned()))
}

pub(crate) fn resolve_project_path(path_arg: Option<String>) -> PathBuf {
    let Some(path_arg) = path_arg else {
        let path = absolutize(".");
        // 无显式 --path 时优先沿父目录寻找已初始化根目录，让子目录里运行查询命令也能工作。
        if is_sqlite_initialized(&path) {
            return path;
        }
        return find_nearest_sqlite_root(&path).unwrap_or(path);
    };

    let path = absolutize(path_arg);
    if is_sqlite_initialized(&path) {
        return path;
    }
    path
}

pub(crate) fn find_nearest_sqlite_root(start_path: &Path) -> Option<PathBuf> {
    let mut current = start_path.to_path_buf();
    loop {
        if is_sqlite_initialized(&current) {
            return Some(current);
        }
        let parent = current.parent()?;
        // Path::parent 在根目录附近可能返回自身；显式断开，避免异常平台路径导致死循环。
        if parent == current {
            return None;
        }
        current = parent.to_path_buf();
    }
}

pub(crate) fn absolutize(path: impl AsRef<Path>) -> PathBuf {
    let path = path.as_ref();
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    }
}

pub(crate) fn now_ms() -> TimestampMs {
    system_time_ms(SystemTime::now())
}

pub(crate) fn system_time_ms(time: SystemTime) -> TimestampMs {
    time.duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as TimestampMs)
        // 系统时钟早于 epoch 时只用于状态时间戳，降级为 0 比传播错误更稳定。
        .unwrap_or(0)
}
