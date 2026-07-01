//! Shared MCP tool response shapes and output guards.
//!
//! MCP 层把“可恢复状态”尽量包装成普通文本结果；只有安全拒绝或真实故障才用
//! `is_error=true`，否则 agent 很容易在会话早期放弃 RustCodeGraph。

use serde::{Deserialize, Serialize};

use super::{MAX_OUTPUT_LENGTH, PendingFile};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolContent {
    #[serde(rename = "type")]
    pub content_type: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolResult {
    pub content: Vec<ToolContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}

pub fn format_stale_banner(stale: &[PendingFile], now_ms: i64) -> String {
    // 命中的文件如果正在等待 watcher 同步，要放在答案顶部强提示；这类内容可能
    // 真会过期，但不要把 agent 引导回原始文件读取；下一轮批量同步会追上。
    let lines = stale
        .iter()
        .map(|pending| {
            let age_ms = (now_ms - pending.last_seen_ms).max(0);
            let label = if pending.indexing {
                "indexing in progress"
            } else {
                "waiting for next batch sync"
            };
            format!("  - {} (edited {age_ms}ms ago, {label})", pending.path)
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "⚠️ Some files referenced below were edited since the last index sync - \
their rustcodegraph entries may be waiting for the next batch sync:\n{lines}\nThe \
watcher is batching changes and will refresh the graph automatically. Treat only \
these entries as possibly stale until that batch sync completes; the rest of this \
response is fresh."
    )
}

pub fn format_stale_footer(stale: &[PendingFile], now_ms: i64) -> String {
    // 未命中的 pending 文件只放脚注，避免把“项目里有别的文件未同步”误读成当前
    // 结果不可用。
    let max = 5;
    let shown = stale.iter().take(max).collect::<Vec<_>>();
    let lines = shown
        .iter()
        .map(|pending| {
            let age_ms = (now_ms - pending.last_seen_ms).max(0);
            format!("  - {} (edited {age_ms}ms ago)", pending.path)
        })
        .collect::<Vec<_>>()
        .join("\n");
    let more = if stale.len() > max {
        format!("\n  - ...and {} more", stale.len() - max)
    } else {
        String::new()
    };
    format!(
        "(Note: {} file(s) elsewhere in this project are waiting for the next \
batch sync and were not referenced above:\n{lines}{more}\nThe current response can \
still be used while the watcher batches those changes.)",
        stale.len()
    )
}

pub fn format_degraded_banner(reason: Option<&str>) -> String {
    // watcher 降级表示索引整体被冻结，这比单个 pending 文件更严重，必须在
    // 每个工具结果前显式提醒。
    let mut out = "⚠️ RustCodeGraph auto-sync is DISABLED - live file watching stopped, \
so the index is frozen and any file edited since then is stale here. Read files \
directly to confirm current content before relying on it."
        .to_string();
    if let Some(reason) = reason {
        out.push_str(&format!("\n  Reason: {reason}"));
    }
    out
}

pub(super) fn prepend_text_notice(result: ToolResult, notice: &str) -> ToolResult {
    let Some((first, rest)) = result.content.split_first() else {
        return result;
    };
    if first.content_type != "text" {
        return result;
    }
    let mut content = Vec::with_capacity(result.content.len());
    content.push(ToolContent {
        content_type: "text".to_string(),
        text: format!("{notice}\n\n{}", first.text),
    });
    content.extend(rest.iter().cloned());
    ToolResult { content, ..result }
}

pub(super) fn append_text_notice(result: ToolResult, notice: &str) -> ToolResult {
    let Some((first, rest)) = result.content.split_first() else {
        return result;
    };
    if first.content_type != "text" {
        return result;
    }
    let mut content = Vec::with_capacity(result.content.len());
    content.push(ToolContent {
        content_type: "text".to_string(),
        text: format!("{}\n\n{notice}", first.text),
    });
    content.extend(rest.iter().cloned());
    ToolResult { content, ..result }
}

pub(super) fn current_time_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or(std::time::Duration::ZERO)
        .as_millis() as i64
}

pub fn text_result(text: &str) -> ToolResult {
    ToolResult {
        content: vec![ToolContent {
            content_type: "text".to_string(),
            text: text.to_string(),
        }],
        is_error: None,
    }
}

pub fn error_result(message: &str) -> ToolResult {
    ToolResult {
        content: vec![ToolContent {
            content_type: "text".to_string(),
            text: format!("Error: {message}"),
        }],
        is_error: Some(true),
    }
}

pub fn truncate_output(text: &str) -> String {
    if text.len() <= MAX_OUTPUT_LENGTH {
        return text.to_string();
    }
    truncate_output_to_limit(text, MAX_OUTPUT_LENGTH)
}

pub(super) fn truncate_output_to_limit(text: &str, max_len: usize) -> String {
    if text.len() <= max_len {
        return text.to_string();
    }
    if max_len == 0 {
        return String::new();
    }
    // 输出预算按字节计，但 Rust 字符串必须在 UTF-8 边界截断；之后尽量回退到
    // 最近换行，减少把源码行切成半截的概率。
    let safe_len = char_boundary_before(text, max_len.min(text.len()));
    let truncated = &text[..safe_len];
    let last_newline = truncated.rfind('\n').unwrap_or(safe_len);
    let cut_point = if last_newline > safe_len * 8 / 10 {
        last_newline
    } else {
        safe_len
    };
    format!(
        "{}\n\n... (output truncated)\nUse another rustcodegraph_explore or rustcodegraph_node with exact names for the missing source; do NOT Read.",
        &truncated[..cut_point]
    )
}

pub(super) fn char_boundary_before(text: &str, mut index: usize) -> usize {
    index = index.min(text.len());
    while index > 0 && !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}
