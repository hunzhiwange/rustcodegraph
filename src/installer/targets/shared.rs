//! Helpers shared across installer target translations.
//!
//! These helpers perform surgical filesystem edits and return the action
//! vocabulary used for user-facing reports.
//!
//! 这里的 helper 偏向“窄写入”：只触碰 RustCodeGraph 管理的键或标记块，
//! 并把动作归一成 `WriteAction`，供 CLI 和测试判断幂等性。

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::installer::instructions_template::{
    RUSTCODEGRAPH_INSTRUCTIONS_BLOCK, RUSTCODEGRAPH_SECTION_END, RUSTCODEGRAPH_SECTION_START,
};

use super::types::{FileWrite, WriteAction, file_write};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerConfig {
    #[serde(rename = "type")]
    pub type_name: String,
    pub command: String,
    pub args: Vec<String>,
}

pub fn get_mcp_server_config() -> McpServerConfig {
    McpServerConfig {
        type_name: "stdio".to_owned(),
        command: "rustcodegraph".to_owned(),
        args: vec!["serve".to_owned(), "--mcp".to_owned()],
    }
}

pub fn get_code_graph_permissions() -> Vec<String> {
    [
        "mcp__rustcodegraph__rustcodegraph_explore",
        "mcp__rustcodegraph__rustcodegraph_search",
        "mcp__rustcodegraph__rustcodegraph_node",
        "mcp__rustcodegraph__rustcodegraph_callers",
        "mcp__rustcodegraph__rustcodegraph_callees",
        "mcp__rustcodegraph__rustcodegraph_impact",
        "mcp__rustcodegraph__rustcodegraph_files",
        "mcp__rustcodegraph__rustcodegraph_status",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}

pub fn home_dir() -> PathBuf {
    env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn current_dir() -> PathBuf {
    env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

pub fn path_to_string(path: impl AsRef<Path>) -> String {
    path.as_ref().to_string_lossy().into_owned()
}

pub fn file_exists(path: impl AsRef<Path>) -> bool {
    path.as_ref().exists()
}

pub fn read_text(path: impl AsRef<Path>) -> String {
    fs::read_to_string(path).unwrap_or_default()
}

pub fn read_json_file(path: impl AsRef<Path>) -> Value {
    let path = path.as_ref();
    if !path.exists() {
        return Value::Object(Map::new());
    }
    fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
        .filter(Value::is_object)
        .unwrap_or_else(|| Value::Object(Map::new()))
}

pub fn write_json_file(path: impl AsRef<Path>, data: &Value) -> std::io::Result<()> {
    let content = serde_json::to_string_pretty(data).unwrap_or_else(|_| "{}".to_owned()) + "\n";
    atomic_write_file_sync(path, &content)
}

pub fn atomic_write_file_sync(path: impl AsRef<Path>, content: &str) -> std::io::Result<()> {
    // 先写同目录临时文件再 rename，避免配置写到一半时被 agent 读取到半截 JSON/TOML。
    let path = path.as_ref();
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension(format!(
        "{}tmp.{}",
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| format!("{ext}."))
            .unwrap_or_default(),
        std::process::id()
    ));
    fs::write(&tmp, content)?;
    fs::rename(tmp, path)
}

pub fn json_deep_equal(a: &Value, b: &Value) -> bool {
    a == b
}

pub fn get_nested<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut cur = value;
    for part in path {
        cur = cur.get(*part)?;
    }
    Some(cur)
}

pub fn has_nested(value: &Value, path: &[&str]) -> bool {
    get_nested(value, path).is_some()
}

pub fn mcp_server_value() -> Value {
    serde_json::to_value(get_mcp_server_config()).unwrap_or_else(|_| {
        json!({
            "type": "stdio",
            "command": "rustcodegraph",
            "args": ["serve", "--mcp"]
        })
    })
}

pub fn mcp_json_snippet(server: Value) -> String {
    serde_json::to_string_pretty(&json!({ "mcpServers": { "rustcodegraph": server } }))
        .unwrap_or_else(|_| "{\n  \"mcpServers\": {}\n}".to_owned())
}

pub fn planned_json_mcp_write(path: impl AsRef<Path>, server: Value) -> FileWrite {
    // JSON target 共用的 MCP upsert：只规范 `mcpServers.rustcodegraph`，
    // 其它顶层键和 sibling server 原样保留。
    let path_ref = path.as_ref();
    let mut existing = read_json_file(path_ref);
    let before = get_nested(&existing, &["mcpServers", "rustcodegraph"]);
    let action = if before.is_some_and(|value| json_deep_equal(value, &server)) {
        WriteAction::Unchanged
    } else if before.is_some() || path_ref.exists() {
        WriteAction::Updated
    } else {
        WriteAction::Created
    };
    if action != WriteAction::Unchanged {
        if !existing.is_object() {
            existing = Value::Object(Map::new());
        }
        let root = existing.as_object_mut().expect("root JSON is an object");
        let servers = root
            .entry("mcpServers")
            .or_insert_with(|| Value::Object(Map::new()));
        if !servers.is_object() {
            *servers = Value::Object(Map::new());
        }
        servers
            .as_object_mut()
            .expect("mcpServers JSON is an object")
            .insert("rustcodegraph".to_owned(), server);
        let _ = write_json_file(path_ref, &existing);
    }
    file_write(path_to_string(path_ref), action)
}

pub fn planned_json_mcp_remove(path: impl AsRef<Path>) -> FileWrite {
    let path_ref = path.as_ref();
    if !path_ref.exists() {
        return file_write(path_to_string(path_ref), WriteAction::NotFound);
    }
    let mut existing = read_json_file(path_ref);
    let mut removed = false;
    if let Some(servers) = existing
        .get_mut("mcpServers")
        .and_then(|value| value.as_object_mut())
    {
        removed = servers.remove("rustcodegraph").is_some();
        if servers.is_empty() {
            existing
                .as_object_mut()
                .and_then(|root| root.remove("mcpServers"));
        }
    }
    let action = if removed {
        let _ = write_json_file(path_ref, &existing);
        WriteAction::Removed
    } else {
        WriteAction::NotFound
    };
    file_write(path_to_string(path_ref), action)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkedSectionAction {
    Created,
    Updated,
    Appended,
    Unchanged,
}

pub fn replace_or_append_marked_section(
    path: impl AsRef<Path>,
    body: &str,
    start_marker: &str,
    end_marker: &str,
) -> MarkedSectionAction {
    // 调用方传入 marker，适用于历史上不同说明文件的兼容清理。
    let path = path.as_ref();
    if !path.exists() {
        let _ = atomic_write_file_sync(path, &(body.to_owned() + "\n"));
        return MarkedSectionAction::Created;
    }
    let Ok(content) = fs::read_to_string(path) else {
        return MarkedSectionAction::Updated;
    };
    let Some(start) = content.find(start_marker) else {
        return MarkedSectionAction::Appended;
    };
    let Some(end_rel) = content[start..].find(end_marker) else {
        return MarkedSectionAction::Appended;
    };
    let end = start + end_rel + end_marker.len();
    if &content[start..end] == body {
        MarkedSectionAction::Unchanged
    } else {
        let before = &content[..start];
        let after = &content[end..];
        let _ = atomic_write_file_sync(path, &format!("{before}{body}{after}"));
        MarkedSectionAction::Updated
    }
}

pub fn upsert_instructions_entry(path: impl AsRef<Path>) -> FileWrite {
    let path_ref = path.as_ref();
    let action =
        match replace_or_append_any_marked_section(path_ref, RUSTCODEGRAPH_INSTRUCTIONS_BLOCK) {
            MarkedSectionAction::Created => WriteAction::Created,
            MarkedSectionAction::Updated | MarkedSectionAction::Appended => WriteAction::Updated,
            MarkedSectionAction::Unchanged => WriteAction::Unchanged,
        };
    file_write(path_to_string(path_ref), action)
}

fn replace_or_append_any_marked_section(path: &Path, body: &str) -> MarkedSectionAction {
    // 当前所有说明块都使用统一 marker；找不到 marker 时追加到文件末尾，
    // 找到 marker 时原地替换，保证重复安装 byte-stable。
    if !path.exists() {
        let _ = atomic_write_file_sync(path, &(body.to_owned() + "\n"));
        return MarkedSectionAction::Created;
    }
    let Ok(content) = fs::read_to_string(path) else {
        return MarkedSectionAction::Updated;
    };
    if let Some(start) = content.find(RUSTCODEGRAPH_SECTION_START)
        && let Some(end_rel) = content[start..].find(RUSTCODEGRAPH_SECTION_END)
    {
        let end = start + end_rel + RUSTCODEGRAPH_SECTION_END.len();
        if &content[start..end] == body {
            return MarkedSectionAction::Unchanged;
        }
        let before = &content[..start];
        let after = &content[end..];
        let _ = atomic_write_file_sync(path, &format!("{before}{body}{after}"));
        return MarkedSectionAction::Updated;
    }

    let trimmed = content.trim_end();
    let sep = if trimmed.is_empty() { "" } else { "\n\n" };
    let _ = atomic_write_file_sync(path, &format!("{trimmed}{sep}{body}\n"));
    MarkedSectionAction::Appended
}

pub fn remove_marked_section(
    path: impl AsRef<Path>,
    start_marker: &str,
    end_marker: &str,
) -> WriteAction {
    // 移除后若文件只剩空白则删文件；否则用一个空行把前后用户内容接回去。
    let path = path.as_ref();
    if !path.exists() {
        return WriteAction::Kept;
    }
    let Ok(content) = fs::read_to_string(path) else {
        return WriteAction::Kept;
    };
    let Some(start) = content.find(start_marker) else {
        return WriteAction::NotFound;
    };
    let Some(end_rel) = content[start..].find(end_marker) else {
        return WriteAction::NotFound;
    };
    if end_rel == 0 {
        return WriteAction::NotFound;
    }
    let end = start + end_rel + end_marker.len();
    let before = content[..start].trim_end();
    let after = content[end..].trim_start();
    let joined = if before.is_empty() {
        after.to_owned()
    } else if after.is_empty() {
        before.to_owned()
    } else {
        format!("{before}\n\n{after}")
    };
    if joined.trim().is_empty() {
        let _ = fs::remove_file(path);
    } else {
        let _ = atomic_write_file_sync(path, &(joined.trim().to_owned() + "\n"));
    }
    WriteAction::Removed
}

pub fn remove_rustcodegraph_instructions(path: impl AsRef<Path>) -> FileWrite {
    let path_ref = path.as_ref();
    let action = remove_marked_section(
        path_ref,
        RUSTCODEGRAPH_SECTION_START,
        RUSTCODEGRAPH_SECTION_END,
    );
    file_write(path_to_string(path_ref), action)
}
