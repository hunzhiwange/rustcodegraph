//! Input validation and user-facing inactive-project guidance for MCP tools.
//!
//! 这里的校验只处理明确非法的参数；未索引/未加载项目属于可恢复状态，返回普通
//! 文本提示，让 agent 可以继续用内置工具而不是反复调用失败工具。

use serde_json::Value;

use super::{
    MAX_INPUT_LENGTH, MAX_PATH_LENGTH,
    response::{ToolResult, error_result},
};

pub(super) fn validate_required_string(value: Option<&Value>, name: &str) -> Option<ToolResult> {
    // required string 为空时工具无法继续，属于真实参数错误，可以返回 isError。
    let Some(value) = value.and_then(Value::as_str) else {
        return Some(error_result(&format!("{name} must be a non-empty string")));
    };
    if value.is_empty() {
        return Some(error_result(&format!("{name} must be a non-empty string")));
    }
    if value.len() > MAX_INPUT_LENGTH {
        return Some(error_result(&format!(
            "{name} exceeds maximum length of {MAX_INPUT_LENGTH} characters (got {})",
            value.len()
        )));
    }
    None
}

pub(super) fn validate_optional_path(value: Option<&Value>, name: &str) -> Option<ToolResult> {
    // projectPath/file 这类可选路径允许 null；但限制长度，防止超大 payload 被带进
    // 文件系统路径处理。
    let value = value?;
    if value.is_null() {
        return None;
    }
    let Some(value) = value.as_str() else {
        return Some(error_result(&format!("{name} must be a string")));
    };
    if value.len() > MAX_PATH_LENGTH {
        return Some(error_result(&format!(
            "{name} exceeds maximum length of {MAX_PATH_LENGTH} characters (got {})",
            value.len()
        )));
    }
    None
}

pub(super) fn not_indexed_message(searched: Option<&str>) -> String {
    // 索引是用户显式动作；MCP 工具不能在 agent 会话里自动初始化项目。
    let searched = searched.unwrap_or(".");
    format!(
        "No RustCodeGraph project is loaded for this session.\nSearched for a .rustcodegraph/ directory starting from: {searched}\nIf this project is indexed, pass projectPath to the tool call or add --path to the MCP server config. If the project has no index, continue with built-in tools; indexing is the user's decision."
    )
}

pub(super) fn unindexed_project_path_message(project_path: &str) -> String {
    format!(
        "Project path {project_path} isn't indexed.\nRun `rustcodegraph init -i` in that project to enable RustCodeGraph, or continue with built-in tools; indexing is the user's decision."
    )
}
