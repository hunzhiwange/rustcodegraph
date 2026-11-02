//! Claude 配置写入的兼容辅助函数，主要供 installer 测试调用。
//!
//! 真实安装目标已经在 `targets/claude.rs` 中实现；这里保留旧入口以验证 JSON 写入和权限检测行为。

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::targets::claude::write_permissions_entry;
use super::targets::shared::{home_dir, mcp_server_value, path_to_string, read_json_file};
use super::targets::types::{FileWrite, Location, WriteAction, file_write};

pub type InstallLocation = Location;

const MCP_SERVER_KEY: &str = "rustcodegraph";

pub fn write_mcp_config(location: InstallLocation) -> FileWrite {
    let file = mcp_json_path(location);
    upsert_mcp_server(&file, &mcp_server_value())
        .unwrap_or_else(|err| panic!("failed to write {}: {err}", file.display()))
}

/// Fill missing managed server keys while preserving the user's JSONC text and values.
/// Invalid input is returned as an error without writing anything.
pub fn upsert_mcp_server(path: &Path, entry: &Value) -> Result<FileWrite, String> {
    use jsonc_parser::cst::{CstInputValue, CstRootNode};
    fn input(value: &Value) -> CstInputValue {
        match value {
            Value::Null => CstInputValue::Null,
            Value::Bool(v) => CstInputValue::Bool(*v),
            Value::Number(v) => CstInputValue::Number(v.to_string()),
            Value::String(v) => CstInputValue::String(v.clone()),
            Value::Array(v) => CstInputValue::Array(v.iter().map(input).collect()),
            Value::Object(v) => {
                CstInputValue::Object(v.iter().map(|(k, v)| (k.clone(), input(v))).collect())
            }
        }
    }
    let entry = entry
        .as_object()
        .ok_or("server defaults must be an object")?;
    let existed = path.exists();
    let before = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(err) => return Err(err.to_string()),
    };
    let text = if before.trim().is_empty() {
        "{\n}\n"
    } else {
        &before
    };
    let root = CstRootNode::parse(text, &Default::default())
        .map_err(|e| format!("invalid JSONC; nothing changed: {e}"))?;
    let object = root
        .object_value()
        .ok_or("configuration must be an object; nothing changed")?;
    let servers = object
        .object_value_or_create("mcpServers")
        .ok_or("mcpServers must be an object; nothing changed")?;
    let server = servers
        .object_value_or_create(MCP_SERVER_KEY)
        .ok_or("managed server must be an object; nothing changed")?;
    for (key, value) in entry {
        if server.get(key).is_none() {
            server.append(key, input(value));
        }
    }
    let after = root.to_string();
    if after == before {
        return Ok(file_write(path_to_string(path), WriteAction::Unchanged));
    }
    super::targets::shared::atomic_write_file_sync(path, &after).map_err(|e| e.to_string())?;
    Ok(file_write(
        path_to_string(path),
        if existed {
            WriteAction::Updated
        } else {
            WriteAction::Created
        },
    ))
}

pub fn write_permissions(location: InstallLocation) -> FileWrite {
    write_permissions_entry(location)
}

pub fn has_mcp_config(location: InstallLocation) -> bool {
    let file = mcp_json_path(location);
    let _path = path_to_string(&file);
    read_json_file(file)
        .pointer(&format!("/mcpServers/{MCP_SERVER_KEY}"))
        .is_some()
}

pub fn has_permissions(location: InstallLocation) -> bool {
    let file = match location {
        Location::Global => super::targets::shared::home_dir()
            .join(".claude")
            .join("settings.json"),
        Location::Local => super::targets::shared::current_dir()
            .join(".claude")
            .join("settings.json"),
    };
    read_json_file(file)
        .pointer("/permissions/allow")
        .and_then(|value| value.as_array())
        .is_some_and(|values| {
            values.iter().any(|value| {
                value
                    .as_str()
                    .is_some_and(|s| s.starts_with("mcp__rustcodegraph__"))
            })
        })
}

fn mcp_json_path(location: InstallLocation) -> PathBuf {
    match location {
        Location::Global => home_dir().join(".claude.json"),
        Location::Local => super::targets::shared::current_dir().join(".mcp.json"),
    }
}
