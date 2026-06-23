//! Cursor installer target.
//!
//! Cursor 启动 MCP 子进程时 cwd 不可靠，也不总在 initialize 传 rootUri。
//! 因此安装配置必须显式带 `--path`，否则 server 可能索引/服务错目录。

use std::path::PathBuf;

use serde_json::{Value, json};

use super::shared::{
    current_dir, get_mcp_server_config, mcp_json_snippet, path_to_string, planned_json_mcp_remove,
    planned_json_mcp_write, read_text, remove_rustcodegraph_instructions,
};
use super::types::{
    AgentTarget, DetectionResult, FileWrite, InstallOptions, Location, TargetId, WriteAction,
    WriteResult, file_write,
};
use crate::installer::instructions_template::{
    RUSTCODEGRAPH_SECTION_END, RUSTCODEGRAPH_SECTION_START,
};

#[derive(Debug, Clone, Copy, Default)]
pub struct CursorTarget;

fn mcp_json_path(loc: Location) -> PathBuf {
    match loc {
        Location::Global => super::shared::home_dir().join(".cursor").join("mcp.json"),
        Location::Local => current_dir().join(".cursor").join("mcp.json"),
    }
}

fn rules_path() -> PathBuf {
    current_dir()
        .join(".cursor")
        .join("rules")
        .join("rustcodegraph.mdc")
}

const MDC_FRONTMATTER: &str = "---\ndescription: RustCodeGraph MCP usage guide - when to use which tool\nalwaysApply: true\n---";

impl AgentTarget for CursorTarget {
    fn id(&self) -> TargetId {
        TargetId::Cursor
    }

    fn display_name(&self) -> &'static str {
        "Cursor"
    }

    fn docs_url(&self) -> Option<&'static str> {
        Some("https://docs.cursor.com/context/model-context-protocol")
    }

    fn supports_location(&self, _loc: Location) -> bool {
        true
    }

    fn detect(&self, loc: Location) -> DetectionResult {
        let mcp_path = mcp_json_path(loc);
        let config = super::shared::read_json_file(&mcp_path);
        let installed = match loc {
            Location::Global => super::shared::home_dir().join(".cursor").exists(),
            Location::Local => current_dir().join(".cursor").exists(),
        };
        DetectionResult {
            installed,
            already_configured: config.pointer("/mcpServers/rustcodegraph").is_some(),
            config_path: Some(path_to_string(mcp_path)),
        }
    }

    fn install(&self, loc: Location, _opts: InstallOptions) -> WriteResult {
        let mut result = WriteResult::empty();
        result.push(write_mcp_entry(loc));
        if loc == Location::Local {
            for cleanup in remove_rules_entries() {
                if cleanup.action == WriteAction::Removed {
                    result.push(cleanup);
                }
            }
        }
        result.with_notes(["Restart Cursor for MCP changes to take effect."])
    }

    fn uninstall(&self, loc: Location) -> WriteResult {
        let mut result = WriteResult::empty();
        result.push(planned_json_mcp_remove(mcp_json_path(loc)));
        if loc == Location::Local {
            result.files.extend(remove_rules_entries());
        }
        result
    }

    fn print_config(&self, loc: Location) -> String {
        format!(
            "# Add to {}\n\n{}\n",
            path_to_string(mcp_json_path(loc)),
            mcp_json_snippet(build_cursor_mcp_config(loc))
        )
    }

    fn describe_paths(&self, loc: Location) -> Vec<String> {
        if loc == Location::Local {
            vec![
                path_to_string(mcp_json_path(loc)),
                path_to_string(rules_path()),
            ]
        } else {
            vec![path_to_string(mcp_json_path(loc))]
        }
    }
}

pub fn build_cursor_mcp_config(loc: Location) -> Value {
    let base = get_mcp_server_config();
    let mut args = base.args;
    args.push("--path".to_owned());
    // 本地安装写绝对当前目录；全局安装使用 Cursor 的 workspaceFolder 占位符，
    // 让同一用户级配置可随工作区切换。
    args.push(match loc {
        Location::Local => path_to_string(current_dir()),
        Location::Global => "${workspaceFolder}".to_owned(),
    });
    json!({
        "type": base.type_name,
        "command": base.command,
        "args": args,
    })
}

fn write_mcp_entry(loc: Location) -> FileWrite {
    planned_json_mcp_write(mcp_json_path(loc), build_cursor_mcp_config(loc))
}

fn remove_rules_entries() -> Vec<FileWrite> {
    vec![remove_rules_entry_at(rules_path())]
}

fn remove_rules_entry_at(file: PathBuf) -> FileWrite {
    // 新版不再写 Cursor rules；卸载/重装只清理我们管理的整文件或标记块。
    if !file.exists() {
        return file_write(path_to_string(file), WriteAction::NotFound);
    }
    let content = read_text(&file);
    let action = if content.trim() == MDC_FRONTMATTER {
        let _ = std::fs::remove_file(&file);
        WriteAction::Removed
    } else if content.contains(RUSTCODEGRAPH_SECTION_START)
        && content.contains(RUSTCODEGRAPH_SECTION_END)
    {
        remove_rules_marked_block(&file, true)
    } else {
        WriteAction::NotFound
    };
    file_write(path_to_string(file), action)
}

fn remove_rules_marked_block(file: &PathBuf, _rust_markers: bool) -> WriteAction {
    let action = remove_rustcodegraph_instructions(file);
    if action.action != WriteAction::Removed {
        return action.action;
    }
    if !file.exists() {
        return WriteAction::Removed;
    }
    let remaining = read_text(file);
    if remaining.trim() == MDC_FRONTMATTER {
        let _ = std::fs::remove_file(file);
    }
    WriteAction::Removed
}
