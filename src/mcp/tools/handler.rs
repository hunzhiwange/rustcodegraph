//! Tool dispatch and shared MCP response adorners.
//!
//! Handler 统一做参数校验、项目 root 选择、未索引成功形状、staleness banner
//! 和 worktree mismatch notice；具体工具只负责生成主体文本。

use std::collections::HashSet;
use std::env;
use std::path::PathBuf;

use serde_json::Value;

use super::registry::{is_tool_allowed, mcp_tools_env_raw, tools};
use super::response::{
    ToolContent, ToolResult, append_text_notice, current_time_ms, error_result,
    format_degraded_banner, format_stale_banner, format_stale_footer, prepend_text_notice,
    text_result,
};
use super::shared::{find_symbol_matches, normalize_slashes};
use super::validation::{
    not_indexed_message, unindexed_project_path_message, validate_optional_path,
    validate_required_string,
};
use super::{PendingFile, ToolDefinition, ToolHandler, files, graph_query, node, search, status};
use crate::directory::find_nearest_code_graph_root;
use crate::sync::worktree::{
    WorktreeIndexMismatch, detect_worktree_index_mismatch, worktree_mismatch_notice,
};
use crate::types::Node;
use crate::utils::validate_project_path;

impl ToolHandler {
    pub fn new(default_project_loaded: bool) -> Self {
        Self {
            default_project_loaded,
            ..Self::default()
        }
    }

    pub fn set_default_project_hint(&mut self, searched_path: impl Into<String>) {
        self.default_project_hint = Some(searched_path.into());
    }

    pub fn set_default_code_graph(&mut self, cg: &crate::CodeGraph) {
        self.set_default_project_root(cg.get_project_root());
    }

    pub fn set_default_project_root(&mut self, project_root: impl Into<String>) {
        self.default_project_root = Some(project_root.into());
        self.default_project_loaded = true;
        self.worktree_mismatch_cache.clear();
    }

    pub fn set_default_project_loaded(&mut self, loaded: bool) {
        self.default_project_loaded = loaded;
        if !loaded {
            self.default_project_root = None;
            self.worktree_mismatch_cache.clear();
        }
    }

    pub fn set_default_file_count(&mut self, file_count: Option<usize>) {
        self.file_count = file_count;
    }

    pub fn set_catch_up_gate<F>(&mut self, gate: F)
    where
        F: FnOnce() -> Result<(), String> + Send + 'static,
    {
        self.catch_up_gate = Some(std::sync::Arc::new(std::sync::Mutex::new(Some(Box::new(
            gate,
        )))));
    }

    pub fn clear_catch_up_gate(&mut self) {
        self.catch_up_gate = None;
    }

    pub fn has_default_code_graph(&self) -> bool {
        self.default_project_loaded
    }

    pub fn close_all(&mut self) {
        self.default_project_loaded = false;
        self.default_project_root = None;
        self.worktree_mismatch_cache.clear();
        self.clear_catch_up_gate();
    }

    pub fn find_symbol_matches(&mut self, cg: &mut crate::CodeGraph, symbol: &str) -> Vec<Node> {
        find_symbol_matches(cg, symbol)
    }

    pub(super) fn narrow_nodes_to_line(nodes: &mut Vec<Node>, line: u64) {
        if nodes.len() <= 1 {
            return;
        }

        let containing = nodes
            .iter()
            .filter(|node| {
                let end = node.end_line.max(node.start_line);
                line >= node.start_line && line <= end
            })
            .cloned()
            .collect::<Vec<_>>();
        if !containing.is_empty() {
            *nodes = containing;
            return;
        }

        if let Some(nearest) = nodes
            .iter()
            .min_by_key(|node| node.start_line.abs_diff(line))
            .cloned()
        {
            *nodes = vec![nearest];
        }
    }

    pub fn get_tools(&self) -> Vec<ToolDefinition> {
        // 小仓库默认只暴露核心三件套，减少 agent 工具选择噪声；环境变量显式
        // 配置时尊重用户选择。
        let mut visible = super::registry::filter_tools(tools(), self.file_count);
        if let Some(file_count) = self.file_count
            && file_count < 500
            && mcp_tools_env_raw()
                .ok()
                .filter(|s| !s.trim().is_empty())
                .is_none()
        {
            let core = HashSet::from([
                "rustcodegraph_explore".to_string(),
                "rustcodegraph_search".to_string(),
                "rustcodegraph_node".to_string(),
            ]);
            visible.retain(|tool| core.contains(&tool.name));
        }
        visible
    }

    pub fn execute(
        &mut self,
        tool_name: &str,
        args: &serde_json::Map<String, Value>,
    ) -> ToolResult {
        // catch-up gate 在首个上下文工具调用前执行一次，用于测试/运行时等待 watcher
        // 把刚写入的索引状态追上。status 是诊断工具，必须保持只读、低成本。
        if should_catch_up_before_tool(tool_name) {
            self.await_catch_up_gate();
        }

        if !is_tool_allowed(tool_name) {
            return error_result(&format!(
                "Tool {tool_name} is disabled via RUSTCODEGRAPH_MCP_TOOLS"
            ));
        }
        if let Some(err) = validate_optional_path(args.get("projectPath"), "projectPath") {
            return err;
        }
        if let Some(err) = validate_optional_path(args.get("path"), "path") {
            return err;
        }
        if let Some(err) = validate_optional_path(args.get("pattern"), "pattern") {
            return err;
        }
        if let Some(project_path) = args.get("projectPath").and_then(Value::as_str)
            && let Some(message) = validate_project_path(project_path)
            && message.to_lowercase().contains("sensitive")
        {
            return error_result(&message);
        }

        match tool_name {
            "rustcodegraph_search" | "rustcodegraph_explore" => {
                if let Some(err) = validate_required_string(args.get("query"), "query") {
                    return err;
                }
            }
            "rustcodegraph_callers" | "rustcodegraph_callees" | "rustcodegraph_impact" => {
                if let Some(err) = validate_required_string(args.get("symbol"), "symbol") {
                    return err;
                }
            }
            "rustcodegraph_node" => {
                let has_symbol = args
                    .get("symbol")
                    .and_then(Value::as_str)
                    .is_some_and(|s| !s.trim().is_empty());
                let has_file = args
                    .get("file")
                    .and_then(Value::as_str)
                    .is_some_and(|s| !s.trim().is_empty());
                if !has_symbol && !has_file {
                    return error_result("symbol must be a non-empty string");
                }
            }
            "rustcodegraph_status" | "rustcodegraph_files" => {}
            _ => return error_result(&format!("Unknown tool: {tool_name}")),
        }

        let project_path = args.get("projectPath").and_then(Value::as_str);
        let project_root = self.project_root_for(project_path);

        if let (Some(project_path), None) = (project_path, project_root.as_ref()) {
            return text_result(&unindexed_project_path_message(project_path));
        }

        if !self.default_project_loaded && project_path.is_none() {
            return text_result(&not_indexed_message(self.default_project_hint.as_deref()));
        }

        match tool_name {
            "rustcodegraph_status" => status::run(self, project_path),
            "rustcodegraph_search" => {
                search::run(self, args, project_path, project_root.as_deref())
            }
            "rustcodegraph_files" => files::run(self, args, project_path, project_root.as_deref()),
            "rustcodegraph_explore" => {
                super::explore::run(self, args, project_path, project_root.as_deref())
            }
            "rustcodegraph_node" => node::run(self, args, project_path, project_root.as_deref()),
            "rustcodegraph_callers" | "rustcodegraph_callees" | "rustcodegraph_impact" => {
                graph_query::run(self, tool_name, args, project_path, project_root.as_deref())
            }
            _ => self.with_worktree_notice(
                text_result("Rust MCP tools are active for this indexed workspace."),
                project_path,
            ),
        }
    }

    fn await_catch_up_gate(&mut self) {
        let Some(gate) = self.catch_up_gate.take() else {
            return;
        };
        let Ok(mut guard) = gate.lock() else {
            return;
        };
        if let Some(run_gate) = guard.take() {
            let _ = run_gate();
        }
    }

    pub(super) fn worktree_mismatch_for(
        &mut self,
        project_path: Option<&str>,
    ) -> Option<WorktreeIndexMismatch> {
        let start_path = project_path
            .or(self.default_project_hint.as_deref())
            .map(str::to_owned)
            .unwrap_or_else(|| {
                env::current_dir()
                    .unwrap_or_else(|_| PathBuf::from("."))
                    .to_string_lossy()
                    .into_owned()
            });

        if let Some(cached) = self.worktree_mismatch_cache.get(&start_path) {
            return cached.clone();
        }

        let mismatch = self
            .project_root_for(project_path)
            .and_then(|root| detect_worktree_index_mismatch(&start_path, root));
        self.worktree_mismatch_cache
            .insert(start_path, mismatch.clone());
        mismatch
    }

    pub(super) fn project_root_for(&self, project_path: Option<&str>) -> Option<String> {
        if let Some(project_path) = project_path
            && let Some(root) = find_nearest_code_graph_root(project_path)
        {
            return Some(root.to_string_lossy().into_owned());
        }
        self.default_project_root.clone()
    }

    pub(super) fn with_index_state_notice(
        &self,
        result: ToolResult,
        project_root: &str,
        referenced_files: &HashSet<String>,
    ) -> ToolResult {
        // staleness 提示只装饰成功结果；真正错误不再追加二级提示，避免掩盖原因。
        if result.is_error == Some(true) {
            return result;
        }
        let Ok(mut cg) = crate::CodeGraph::open_sync(std::path::Path::new(project_root)) else {
            return result;
        };
        let degraded_reason = cg.get_watcher_degraded_reason();
        let pending = cg
            .get_pending_files()
            .into_iter()
            .map(|pending| PendingFile {
                path: pending.path,
                last_seen_ms: pending.last_seen_ms,
                indexing: pending.indexing,
            })
            .collect::<Vec<_>>();
        cg.close();

        if let Some(reason) = degraded_reason {
            return prepend_text_notice(result, &format_degraded_banner(Some(&reason)));
        }
        if pending.is_empty() {
            return result;
        }

        let response_text = result
            .content
            .first()
            .filter(|content| content.content_type == "text")
            .map(|content| content.text.as_str())
            .unwrap_or("");
        let (referenced, elsewhere): (Vec<_>, Vec<_>) = pending.into_iter().partition(|pending| {
            // 输出里提到的 pending 文件放顶部强提醒；其它 pending 文件放尾部，
            // 让 agent 知道索引正在追赶但当前答案仍可用。
            let path = normalize_slashes(&pending.path);
            referenced_files.contains(&path) || response_text.contains(&path)
        });
        let now_ms = current_time_ms();
        if !referenced.is_empty() {
            prepend_text_notice(result, &format_stale_banner(&referenced, now_ms))
        } else {
            append_text_notice(result, &format_stale_footer(&elsewhere, now_ms))
        }
    }

    pub(super) fn with_worktree_notice(
        &mut self,
        result: ToolResult,
        project_path: Option<&str>,
    ) -> ToolResult {
        if result.is_error == Some(true) {
            return result;
        }
        let Some(mismatch) = self.worktree_mismatch_for(project_path) else {
            return result;
        };
        let Some((first, rest)) = result.content.split_first() else {
            return result;
        };
        if first.content_type != "text" {
            return result;
        }

        let mut content = Vec::with_capacity(result.content.len());
        content.push(ToolContent {
            content_type: "text".to_string(),
            text: format!("{}\n\n{}", worktree_mismatch_notice(&mismatch), first.text),
        });
        content.extend(rest.iter().cloned());
        ToolResult { content, ..result }
    }
}

fn should_catch_up_before_tool(tool_name: &str) -> bool {
    !matches!(tool_name, "rustcodegraph_status")
}

#[cfg(test)]
mod tests {
    use super::should_catch_up_before_tool;

    #[test]
    fn status_is_diagnostic_and_does_not_trigger_catch_up() {
        assert!(!should_catch_up_before_tool("rustcodegraph_status"));
        assert!(should_catch_up_before_tool("rustcodegraph_search"));
        assert!(should_catch_up_before_tool("rustcodegraph_explore"));
    }
}
