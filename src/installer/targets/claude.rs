//! Claude Code installer target.
//!
//! Claude 同时有 MCP 配置、settings 权限和说明文件三类状态。这里保持
//! “追加 RustCodeGraph 自己的块/权限，保留用户其它配置”的安装契约。

use std::path::PathBuf;

use super::shared::{
    current_dir, get_code_graph_permissions, mcp_json_snippet, mcp_server_value, path_to_string,
    planned_json_mcp_remove, planned_json_mcp_write, read_json_file,
    remove_rustcodegraph_instructions, upsert_instructions_entry, write_json_file,
};
use super::types::{
    AgentTarget, DetectionResult, FileWrite, InstallOptions, Location, TargetId, WriteAction,
    WriteResult, file_write,
};

#[derive(Debug, Clone, Copy, Default)]
pub struct ClaudeTarget;

fn config_dir(loc: Location) -> PathBuf {
    match loc {
        Location::Global => super::shared::home_dir().join(".claude"),
        Location::Local => current_dir().join(".claude"),
    }
}

fn mcp_json_path(loc: Location) -> PathBuf {
    match loc {
        Location::Global => super::shared::home_dir().join(".claude.json"),
        Location::Local => current_dir().join(".mcp.json"),
    }
}

fn legacy_local_mcp_path() -> PathBuf {
    // 早期本地安装曾写 `.claude.json`；当前本地 MCP 配置使用 `.mcp.json`。
    current_dir().join(".claude.json")
}

fn settings_json_path(loc: Location) -> PathBuf {
    config_dir(loc).join("settings.json")
}

fn instructions_path(loc: Location) -> PathBuf {
    config_dir(loc).join("CLAUDE.md")
}

impl AgentTarget for ClaudeTarget {
    fn id(&self) -> TargetId {
        TargetId::Claude
    }

    fn display_name(&self) -> &'static str {
        "Claude Code"
    }

    fn docs_url(&self) -> Option<&'static str> {
        Some("https://docs.claude.com/en/docs/claude-code")
    }

    fn supports_location(&self, _loc: Location) -> bool {
        true
    }

    fn detect(&self, loc: Location) -> DetectionResult {
        let mcp_path = mcp_json_path(loc);
        let config = read_json_file(&mcp_path);
        let already_configured = config.pointer("/mcpServers/rustcodegraph").is_some();
        let installed = match loc {
            Location::Global => config_dir(loc).exists() || mcp_path.exists(),
            Location::Local => mcp_path.exists() || config_dir(loc).exists(),
        };
        DetectionResult {
            installed,
            already_configured,
            config_path: Some(path_to_string(mcp_path)),
        }
    }

    fn install(&self, loc: Location, opts: InstallOptions) -> WriteResult {
        let mut result = WriteResult::empty();
        result.push(write_mcp_entry(loc));
        if loc == Location::Local
            && let Some(file) = cleanup_legacy_local_mcp()
        {
            result.push(file);
        }
        if opts.auto_allow {
            result.push(write_permissions_entry(loc));
        }
        let hook_cleanup = cleanup_legacy_hooks(loc);
        if hook_cleanup.action == WriteAction::Removed {
            result.push(hook_cleanup);
        }
        result.push(upsert_instructions_entry(instructions_path(loc)));
        result
    }

    fn uninstall(&self, loc: Location) -> WriteResult {
        let mut result = WriteResult::empty();
        result.push(planned_json_mcp_remove(mcp_json_path(loc)));
        if loc == Location::Local
            && let Some(file) = cleanup_legacy_local_mcp()
        {
            result.push(file);
        }
        result.push(remove_permissions_entry(loc));
        let hook_cleanup = cleanup_legacy_hooks(loc);
        if hook_cleanup.action == WriteAction::Removed {
            result.push(hook_cleanup);
        }
        result.push(remove_instructions_entry(loc));
        result
    }

    fn print_config(&self, loc: Location) -> String {
        format!(
            "# Add to {}\n\n{}\n",
            path_to_string(mcp_json_path(loc)),
            mcp_json_snippet(mcp_server_value())
        )
    }

    fn describe_paths(&self, loc: Location) -> Vec<String> {
        vec![
            path_to_string(mcp_json_path(loc)),
            path_to_string(settings_json_path(loc)),
            path_to_string(instructions_path(loc)),
        ]
    }
}

pub fn write_mcp_entry(loc: Location) -> FileWrite {
    planned_json_mcp_write(mcp_json_path(loc), mcp_server_value())
}

fn cleanup_legacy_local_mcp() -> Option<FileWrite> {
    // 迁移只移除 rustcodegraph 这一项；如果 legacy 文件还含其它 server，
    // 文件必须保留，防止卸载/重装破坏用户配置。
    let file = legacy_local_mcp_path();
    if !file.exists() {
        return None;
    }
    let mut config = read_json_file(&file);
    let servers = config
        .get_mut("mcpServers")
        .and_then(|value| value.as_object_mut())?;
    let removed = servers.remove("rustcodegraph").is_some();
    if !removed {
        return None;
    }
    if servers.is_empty() {
        config
            .as_object_mut()
            .and_then(|root| root.remove("mcpServers"));
    }
    if config.as_object().is_some_and(|root| root.is_empty()) {
        let _ = std::fs::remove_file(&file);
    } else {
        let _ = write_json_file(&file, &config);
    }
    Some(file_write(path_to_string(file), WriteAction::Removed))
}

fn is_rustcodegraph_hook_command(command: &str) -> bool {
    // 旧版通过 Claude hooks 做 dirty/sync，现在由 watcher/CLI 负责；
    // 命令可能被用户串在 shell 管道里，所以按常见分隔符拆分后识别。
    command
        .split([';', '&', '|'])
        .map(str::trim_start)
        .any(|part| {
            matches!(
                part,
                "rustcodegraph mark-dirty" | "rustcodegraph sync-if-dirty"
            ) || part.starts_with("rustcodegraph mark-dirty ")
                || part.starts_with("rustcodegraph sync-if-dirty ")
        })
}

pub fn cleanup_legacy_hooks(loc: Location) -> FileWrite {
    let file = settings_json_path(loc);
    if !file.exists() {
        return file_write(path_to_string(file), WriteAction::NotFound);
    }
    let mut settings = read_json_file(&file);
    let mut changed = false;
    if let Some(hooks) = settings
        .get_mut("hooks")
        .and_then(|value| value.as_object_mut())
    {
        let events = hooks.keys().cloned().collect::<Vec<_>>();
        for event in events {
            let Some(groups) = hooks.get_mut(&event).and_then(|value| value.as_array_mut()) else {
                continue;
            };
            let mut next_groups = Vec::new();
            for mut group in std::mem::take(groups) {
                if let Some(commands) = group
                    .get_mut("hooks")
                    .and_then(|value| value.as_array_mut())
                {
                    let before = commands.len();
                    commands.retain(|hook| {
                        !hook
                            .get("command")
                            .and_then(|value| value.as_str())
                            .is_some_and(is_rustcodegraph_hook_command)
                    });
                    if commands.len() != before {
                        changed = true;
                    }
                    if commands.is_empty() {
                        changed = true;
                        continue;
                    }
                }
                next_groups.push(group);
            }
            if next_groups.is_empty() {
                hooks.remove(&event);
                changed = true;
            } else {
                hooks.insert(event, serde_json::Value::Array(next_groups));
            }
        }
        if hooks.is_empty() {
            settings
                .as_object_mut()
                .and_then(|root| root.remove("hooks"));
        }
    }
    let action = if changed {
        let _ = write_json_file(&file, &settings);
        WriteAction::Removed
    } else {
        WriteAction::Unchanged
    };
    file_write(path_to_string(file), action)
}

pub fn write_permissions_entry(loc: Location) -> FileWrite {
    // 权限列表采用“补齐缺项”的方式，既能幂等重跑，也不重排用户已有 allow。
    let file = settings_json_path(loc);
    let mut config = read_json_file(&file);
    let want = get_code_graph_permissions();
    let existing = config
        .pointer("/permissions/allow")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default();
    let all_present = want.iter().all(|perm| {
        existing
            .iter()
            .any(|value| value.as_str() == Some(perm.as_str()))
    });
    let action = if all_present && file.exists() {
        WriteAction::Unchanged
    } else if file.exists() {
        WriteAction::Updated
    } else {
        WriteAction::Created
    };
    if action != WriteAction::Unchanged {
        if !config.is_object() {
            config = serde_json::Value::Object(serde_json::Map::new());
        }
        let root = config.as_object_mut().expect("settings root is an object");
        let permissions = root
            .entry("permissions")
            .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
        if !permissions.is_object() {
            *permissions = serde_json::Value::Object(serde_json::Map::new());
        }
        let permissions = permissions
            .as_object_mut()
            .expect("permissions is an object");
        let allow = permissions
            .entry("allow")
            .or_insert_with(|| serde_json::Value::Array(Vec::new()));
        if !allow.is_array() {
            *allow = serde_json::Value::Array(Vec::new());
        }
        let allow = allow.as_array_mut().expect("allow is an array");
        for permission in want {
            if !allow
                .iter()
                .any(|value| value.as_str() == Some(permission.as_str()))
            {
                allow.push(serde_json::Value::String(permission));
            }
        }
        let _ = write_json_file(&file, &config);
    }
    file_write(path_to_string(file), action)
}

fn remove_permissions_entry(loc: Location) -> FileWrite {
    let file = settings_json_path(loc);
    let mut config = read_json_file(&file);
    let mut removed = false;
    if let Some(permissions) = config
        .get_mut("permissions")
        .and_then(|value| value.as_object_mut())
    {
        if let Some(allow) = permissions
            .get_mut("allow")
            .and_then(|value| value.as_array_mut())
        {
            let before = allow.len();
            allow.retain(|value| {
                !value
                    .as_str()
                    .is_some_and(|s| s.starts_with("mcp__rustcodegraph__"))
            });
            removed = allow.len() != before;
            if allow.is_empty() {
                permissions.remove("allow");
            }
        }
        if permissions.is_empty() {
            config
                .as_object_mut()
                .and_then(|root| root.remove("permissions"));
        }
    }
    let action = if removed {
        let _ = write_json_file(&file, &config);
        WriteAction::Removed
    } else {
        WriteAction::NotFound
    };
    file_write(path_to_string(file), action)
}

pub fn remove_instructions_entry(loc: Location) -> FileWrite {
    remove_rustcodegraph_instructions(instructions_path(loc))
}
