//! Kiro CLI / IDE installer target.
//!
//! Kiro 的 MCP 配置是 JSON，旧版还写过 steering 文档。当前安装只需要
//! MCP 配置，并在重装时清理旧 steering 文件以免 agent 收到重复指导。

use std::path::PathBuf;

use super::shared::{
    current_dir, mcp_json_snippet, mcp_server_value, path_to_string, planned_json_mcp_remove,
    planned_json_mcp_write, read_json_file,
};
use super::types::{
    AgentTarget, DetectionResult, FileWrite, InstallOptions, Location, TargetId, WriteAction,
    WriteResult, file_write,
};

#[derive(Debug, Clone, Copy, Default)]
pub struct KiroTarget;

fn config_dir(loc: Location) -> PathBuf {
    match loc {
        Location::Global => super::shared::home_dir().join(".kiro"),
        Location::Local => current_dir().join(".kiro"),
    }
}

fn mcp_json_path(loc: Location) -> PathBuf {
    config_dir(loc).join("settings").join("mcp.json")
}

fn steering_path(loc: Location) -> PathBuf {
    config_dir(loc).join("steering").join("rustcodegraph.md")
}

impl AgentTarget for KiroTarget {
    fn id(&self) -> TargetId {
        TargetId::Kiro
    }

    fn display_name(&self) -> &'static str {
        "Kiro"
    }

    fn docs_url(&self) -> Option<&'static str> {
        Some("https://kiro.dev/docs/cli/mcp/")
    }

    fn supports_location(&self, _loc: Location) -> bool {
        true
    }

    fn detect(&self, loc: Location) -> DetectionResult {
        let file = mcp_json_path(loc);
        let config = read_json_file(&file);
        let installed = match loc {
            Location::Global => config_dir(Location::Global).exists() || file.exists(),
            Location::Local => file.exists() || config_dir(Location::Local).exists(),
        };
        DetectionResult {
            installed,
            already_configured: config.pointer("/mcpServers/rustcodegraph").is_some(),
            config_path: Some(path_to_string(file)),
        }
    }

    fn install(&self, loc: Location, _opts: InstallOptions) -> WriteResult {
        let mut result = WriteResult::empty();
        result.push(write_mcp_entry(loc));
        let steering = remove_steering_entry(loc);
        if steering.action == WriteAction::Removed {
            result.push(steering);
        }
        result.with_notes([
            "Restart Kiro for MCP changes to take effect.",
            "Kiro IDE: also enable MCP in Settings (search \"MCP\" -> \"Enabled\"). Kiro CLI users can skip this step.",
        ])
    }

    fn uninstall(&self, loc: Location) -> WriteResult {
        let mut result = WriteResult::empty();
        result.push(planned_json_mcp_remove(mcp_json_path(loc)));
        result.push(remove_steering_entry(loc));
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
            path_to_string(steering_path(loc)),
        ]
    }
}

fn write_mcp_entry(loc: Location) -> FileWrite {
    planned_json_mcp_write(mcp_json_path(loc), mcp_server_value())
}

fn remove_steering_entry(loc: Location) -> FileWrite {
    // steering 文件是 RustCodeGraph 早期独占生成物，可以整文件删除。
    let file = steering_path(loc);
    let action = if file.exists() {
        let _ = std::fs::remove_file(&file);
        WriteAction::Removed
    } else {
        WriteAction::NotFound
    };
    file_write(path_to_string(file), action)
}
