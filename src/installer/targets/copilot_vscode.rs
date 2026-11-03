//! VS Code MCP configuration (VS Code 1.102+); no instructions files.
use super::shared::{current_dir, home_dir, mcp_server_value, path_to_string, read_text};
use super::types::*;
use serde_json::{Value, json};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, Default)]
pub struct CopilotVscodeTarget;

fn user_dir() -> PathBuf {
    if cfg!(target_os = "macos") {
        home_dir().join("Library/Application Support/Code/User")
    } else if cfg!(windows) {
        super::shared::config_dir_from_env("APPDATA", home_dir().join("AppData/Roaming"))
            .join("Code/User")
    } else {
        super::shared::config_dir_from_env("XDG_CONFIG_HOME", home_dir().join(".config"))
            .join("Code/User")
    }
}
fn config_path(loc: Location) -> PathBuf {
    match loc {
        Location::Global => user_dir().join("mcp.json"),
        Location::Local => current_dir().join(".vscode/mcp.json"),
    }
}
fn entry(loc: Location) -> Value {
    let mut value = mcp_server_value();
    if loc == Location::Local {
        value["args"] = json!(["serve", "--mcp", "--path", path_to_string(current_dir())]);
    }
    value
}
impl AgentTarget for CopilotVscodeTarget {
    fn id(&self) -> TargetId {
        TargetId::CopilotVscode
    }
    fn display_name(&self) -> &'static str {
        "VS Code (Copilot Chat)"
    }
    fn docs_url(&self) -> Option<&'static str> {
        Some("https://code.visualstudio.com/docs/copilot/customization/mcp-servers")
    }
    fn supports_location(&self, _: Location) -> bool {
        true
    }
    fn detect(&self, loc: Location) -> DetectionResult {
        let path = config_path(loc);
        let config = jsonc_parser::parse_to_serde_value(&read_text(&path), &Default::default())
            .ok()
            .flatten();
        DetectionResult {
            installed: match loc {
                Location::Global => user_dir().exists() || home_dir().join(".vscode").exists(),
                Location::Local => current_dir().join(".vscode").exists(),
            },
            already_configured: config
                .as_ref()
                .and_then(|v| v.pointer("/servers/rustcodegraph"))
                .is_some(),
            config_path: Some(path_to_string(path)),
        }
    }
    fn install(&self, loc: Location, _: InstallOptions) -> WriteResult {
        let path = config_path(loc);
        let file = crate::installer::config_writer::upsert_mcp_server_under(
            &path,
            "servers",
            &entry(loc),
            true,
        )
        .unwrap_or_else(|e| panic!("failed to write {}: {e}", path.display()));
        WriteResult::with_file(file).with_notes(["Restart VS Code for MCP changes to take effect."])
    }
    fn uninstall(&self, loc: Location) -> WriteResult {
        let path = config_path(loc);
        let unchanged =
            || WriteResult::with_file(file_write(path_to_string(&path), WriteAction::NotFound));
        if !path.exists() {
            return unchanged();
        }
        let root = jsonc_parser::cst::CstRootNode::parse(&read_text(&path), &Default::default())
            .expect("invalid VS Code JSONC");
        let Some(object) = root.object_value() else {
            return unchanged();
        };
        let Some(servers) = object.get("servers").and_then(|p| p.object_value()) else {
            return unchanged();
        };
        let Some(server) = servers.get("rustcodegraph") else {
            return unchanged();
        };
        server.remove();
        if servers.properties().is_empty() {
            object.get("servers").unwrap().remove();
        }
        super::shared::atomic_write_file_sync(&path, &root.to_string())
            .expect("write VS Code config");
        WriteResult::with_file(file_write(path_to_string(path), WriteAction::Removed))
    }
    fn print_config(&self, loc: Location) -> String {
        format!(
            "# Add to {}\n\n{}\n",
            path_to_string(config_path(loc)),
            serde_json::to_string_pretty(&json!({"servers": {"rustcodegraph": entry(loc)}}))
                .unwrap()
        )
    }
    fn describe_paths(&self, loc: Location) -> Vec<String> {
        vec![path_to_string(config_path(loc))]
    }
}
