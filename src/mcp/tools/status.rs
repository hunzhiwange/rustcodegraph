//! Implementation of `rustcodegraph_status`.
//!
//! status 是诊断工具，不参与常规上下文检索；输出重点是索引健康、watcher 降级和
//! worktree mismatch，方便用户判断 agent 拿到的图是否可信。

use std::path::Path;

use super::ToolHandler;
use super::response::{ToolResult, current_time_ms, text_result};
use crate::sync::worktree::worktree_mismatch_warning;

pub(super) fn run(handler: &mut ToolHandler, project_path: Option<&str>) -> ToolResult {
    let mut lines = vec!["## RustCodeGraph Status".to_string(), String::new()];

    if let Some(mismatch) = handler.worktree_mismatch_for(project_path) {
        // worktree mismatch 会导致索引内容和当前编辑树错位，比普通 stale 更难察觉。
        lines.push(format!(
            "> ⚠ {}",
            worktree_mismatch_warning(&mismatch).replace('\n', "\n> ")
        ));
        lines.push(String::new());
    }

    if let Some(project_root) = handler.project_root_for(project_path)
        && let Ok(mut cg) = crate::CodeGraph::open_sync(Path::new(&project_root))
    {
        // status 直接打开 DB 读取摘要；失败时保持成功形状的简短结果，避免诊断工具
        // 把会话带入 error-abandonment 路径。
        let stats = cg.get_stats();
        lines.push(format!("Project root: `{project_root}`"));
        lines.push(format!("Backend: {}", cg.get_backend()));
        let journal = cg.get_journal_mode();
        if !journal.is_empty() {
            lines.push(format!("Journal mode: {journal}"));
        }
        lines.push(format!(
            "Indexed: {} files, {} nodes, {} edges",
            stats.file_count, stats.node_count, stats.edge_count
        ));
        lines.push(String::new());

        if let Some(reason) = cg.get_watcher_degraded_reason() {
            lines.push("### Auto-sync disabled:".to_string());
            lines.push(reason);
            lines.push(String::new());
        }

        let pending = cg.get_pending_files();
        if !pending.is_empty() {
            lines.push("### Pending sync:".to_string());
            lines.push(
                "Watcher is waiting for the next batch sync; these graph entries may be stale until it completes."
                    .to_string(),
            );
            let now_ms = current_time_ms();
            for pending in pending {
                let age_ms = (now_ms - pending.last_seen_ms).max(0);
                let state = if pending.indexing {
                    "indexing in progress"
                } else {
                    "waiting for batch sync"
                };
                lines.push(format!(
                    "- {} (edited {age_ms}ms ago, {state})",
                    pending.path
                ));
            }
            lines.push(String::new());
        }
        cg.close();
    }

    lines.push("Rust MCP tools are active for this indexed workspace.".to_string());
    text_result(&lines.join("\n"))
}
