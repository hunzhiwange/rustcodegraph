//! Interactive daemon-manager logic translated from `daemon-manager.ts`.
//!
//! 这个模块只表达“列出 daemon -> 用户选择 -> 停止”的交互状态机。
//! 真实输入输出通过 `PickerDeps` 注入，方便 CLI 和测试复用同一套排序/文案逻辑。

use std::path::{Path, PathBuf};

use super::daemon_registry::{DaemonRecord, StopOutcome, StopResult};

pub const STOP_ALL: &str = "__stop_all__";
pub const CANCEL: &str = "__cancel__";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PickItem {
    pub value: String,
    pub label: String,
    pub hint: Option<String>,
}

pub fn format_uptime(ms: i64) -> String {
    let seconds = ms.max(0) / 1000;
    if seconds < 60 {
        return format!("{seconds}s");
    }
    let minutes = seconds / 60;
    if minutes < 60 {
        return format!("{minutes}m");
    }
    let hours = minutes / 60;
    format!("{hours}h {}m", minutes % 60)
}

pub fn build_pick_items(
    daemons: &[DaemonRecord],
    cwd_root: Option<&Path>,
    now_ms: i64,
) -> Vec<PickItem> {
    // 当前项目排在最前，其余按启动时间倒序；用户管理多个工作区 daemon 时
    // 最常见的操作是停止当前项目。
    let cwd = cwd_root.map(resolve_lossy);
    let mut ordered = daemons.to_vec();
    ordered.sort_by(|a, b| {
        if let Some(cwd) = &cwd {
            let a_current = resolve_lossy(Path::new(&a.root)) == *cwd;
            let b_current = resolve_lossy(Path::new(&b.root)) == *cwd;
            if a_current != b_current {
                return b_current.cmp(&a_current);
            }
        }
        b.started_at.cmp(&a.started_at)
    });

    let mut items = Vec::new();
    for daemon in ordered {
        let current = cwd
            .as_ref()
            .is_some_and(|cwd| resolve_lossy(Path::new(&daemon.root)) == *cwd);
        items.push(PickItem {
            value: daemon.root.clone(),
            label: if current {
                format!("{}  (current project)", daemon.root)
            } else {
                daemon.root.clone()
            },
            hint: Some(format!(
                "pid {} - up {} - Running",
                daemon.pid,
                format_uptime(now_ms - daemon.started_at)
            )),
        });
    }

    if items.len() > 1 {
        items.push(PickItem {
            value: STOP_ALL.to_string(),
            label: "Stop all".to_string(),
            hint: Some(String::new()),
        });
    }
    items.push(PickItem {
        value: CANCEL.to_string(),
        label: "Cancel".to_string(),
        hint: Some(String::new()),
    });
    items
}

pub trait PickerDeps {
    fn list(&self) -> Vec<DaemonRecord>;
    fn stop(&mut self, root: &str) -> StopResult;
    fn stop_all(&mut self) -> Vec<StopResult>;
    fn cwd_root(&self) -> Option<PathBuf>;
    fn now(&self) -> i64;
    fn select(&mut self, items: &[PickItem], initial_value: &str) -> Option<String>;
    fn note(&mut self, msg: &str);
    fn done(&mut self, msg: &str);
}

pub fn run_daemon_picker<D: PickerDeps>(deps: &mut D) {
    // 每次停止单个 daemon 后重新拉列表，避免列表中的 pid/root 已被其它进程改变。
    loop {
        let daemons = deps.list();
        if daemons.is_empty() {
            deps.done("All daemons stopped.");
            return;
        }

        let cwd_root = deps.cwd_root();
        let items = build_pick_items(&daemons, cwd_root.as_deref(), deps.now());
        let initial = items
            .first()
            .map(|item| item.value.as_str())
            .unwrap_or(CANCEL);
        let choice = deps.select(&items, initial);
        let Some(choice) = choice else {
            deps.done("Cancelled.");
            return;
        };
        if choice == CANCEL {
            deps.done("Cancelled.");
            return;
        }
        if choice == STOP_ALL {
            let results = deps.stop_all();
            let n = results
                .iter()
                .filter(|result| matches!(result.outcome, StopOutcome::Term | StopOutcome::Kill))
                .count();
            deps.note(&format!(
                "Stopped {n} daemon{}.",
                if n == 1 { "" } else { "s" }
            ));
            deps.done("Done.");
            return;
        }

        let result = deps.stop(&choice);
        let forced = if result.outcome == StopOutcome::Kill {
            ", forced"
        } else {
            ""
        };
        deps.note(&format!(
            "Stopped daemon (pid {}{forced}) - {choice}",
            result
                .pid
                .map(|pid| pid.to_string())
                .unwrap_or_else(|| "?".to_string())
        ));
    }
}

fn resolve_lossy(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}
