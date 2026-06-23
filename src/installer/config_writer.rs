//! Claude 配置写入的兼容辅助函数，主要供 installer 测试调用。
//!
//! 真实安装目标已经在 `targets/claude.rs` 中实现；这里保留旧入口以验证 JSON 写入和权限检测行为。

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use super::targets::claude::write_permissions_entry;
use super::targets::shared::{
    home_dir, json_deep_equal, mcp_server_value, path_to_string, read_json_file, write_json_file,
};
use super::targets::types::{FileWrite, Location, WriteAction, file_write};

pub type InstallLocation = Location;

const MCP_SERVER_KEY: &str = "rustcodegraph";

pub fn write_mcp_config(location: InstallLocation) -> FileWrite {
    let file = mcp_json_path(location);
    let mut config = read_json_file_for_write(&file);
    let after = mcp_server_value();
    let pointer = format!("/mcpServers/{MCP_SERVER_KEY}");
    let before = config.pointer(&pointer);

    if before.is_some_and(|value| json_deep_equal(value, &after)) {
        return file_write(path_to_string(file), WriteAction::Unchanged);
    }

    let action = if before.is_some() || file.exists() {
        WriteAction::Updated
    } else {
        WriteAction::Created
    };

    if !config.is_object() {
        config = Value::Object(Map::new());
    }
    // 即使用户文件里 mcpServers 不是对象，也只重建这一节，尽量保留其他顶层配置。
    let root = config
        .as_object_mut()
        .expect("config should be normalized to a JSON object");
    let servers = root
        .entry("mcpServers".to_owned())
        .or_insert_with(|| Value::Object(Map::new()));
    if !servers.is_object() {
        *servers = Value::Object(Map::new());
    }
    servers
        .as_object_mut()
        .expect("mcpServers should be normalized to a JSON object")
        .insert(MCP_SERVER_KEY.to_owned(), after);

    write_json_file(&file, &config)
        .unwrap_or_else(|err| panic!("failed to write {}: {err}", file.display()));
    file_write(path_to_string(file), action)
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

fn read_json_file_for_write(path: &Path) -> Value {
    if !path.exists() {
        return Value::Object(Map::new());
    }

    let Ok(raw) = fs::read_to_string(path) else {
        return Value::Object(Map::new());
    };

    match serde_json::from_str::<Value>(&raw) {
        Ok(Value::Object(map)) => Value::Object(map),
        Ok(_) => Value::Object(Map::new()),
        Err(err) => {
            // 解析失败时先备份原文件再覆盖，避免安装器把用户手写配置直接吞掉。
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .map(str::to_owned)
                .unwrap_or_else(|| path.to_string_lossy().into_owned());
            eprintln!("  Warning: Could not parse {name}: {err}");
            eprintln!("  A backup will be created before overwriting.");
            let mut backup = path.to_path_buf();
            backup.as_mut_os_string().push(".backup");
            let _ = fs::write(backup, raw);
            Value::Object(Map::new())
        }
    }
}
