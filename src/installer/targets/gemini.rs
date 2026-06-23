//! Gemini CLI installer target.
//!
//! Gemini 的 MCP 配置和说明文件都支持 local/global。路径函数集中在文件顶部，
//! 让 detect/install/uninstall 始终引用同一套位置，减少跨平台路径漂移。

use std::path::PathBuf;

use super::shared::{
    current_dir, mcp_json_snippet, mcp_server_value, path_to_string, planned_json_mcp_remove,
    planned_json_mcp_write, read_json_file, remove_rustcodegraph_instructions,
    upsert_instructions_entry,
};
use super::types::{
    AgentTarget, DetectionResult, FileWrite, InstallOptions, Location, TargetId, WriteResult,
};

#[derive(Debug, Clone, Copy, Default)]
pub struct GeminiTarget;

fn config_dir(loc: Location) -> PathBuf {
    match loc {
        Location::Global => super::shared::home_dir().join(".gemini"),
        Location::Local => current_dir().join(".gemini"),
    }
}

fn settings_json_path(loc: Location) -> PathBuf {
    config_dir(loc).join("settings.json")
}

fn instructions_path(loc: Location) -> PathBuf {
    match loc {
        Location::Global => config_dir(Location::Global).join("GEMINI.md"),
        Location::Local => current_dir().join("GEMINI.md"),
    }
}

impl AgentTarget for GeminiTarget {
    fn id(&self) -> TargetId {
        TargetId::Gemini
    }

    fn display_name(&self) -> &'static str {
        "Gemini CLI"
    }

    fn docs_url(&self) -> Option<&'static str> {
        Some("https://geminicli.com/docs/tools/mcp-server/")
    }

    fn supports_location(&self, _loc: Location) -> bool {
        true
    }

    fn detect(&self, loc: Location) -> DetectionResult {
        let file = settings_json_path(loc);
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
        result.push(upsert_instructions_entry(instructions_path(loc)));
        result
    }

    fn uninstall(&self, loc: Location) -> WriteResult {
        let mut result = WriteResult::empty();
        result.push(planned_json_mcp_remove(settings_json_path(loc)));
        result.push(remove_instructions_entry(loc));
        result
    }

    fn print_config(&self, loc: Location) -> String {
        format!(
            "# Add to {}\n\n{}\n",
            path_to_string(settings_json_path(loc)),
            mcp_json_snippet(mcp_server_value())
        )
    }

    fn describe_paths(&self, loc: Location) -> Vec<String> {
        vec![
            path_to_string(settings_json_path(loc)),
            path_to_string(instructions_path(loc)),
        ]
    }
}

fn write_mcp_entry(loc: Location) -> FileWrite {
    planned_json_mcp_write(settings_json_path(loc), mcp_server_value())
}

fn remove_instructions_entry(loc: Location) -> FileWrite {
    remove_rustcodegraph_instructions(instructions_path(loc))
}
