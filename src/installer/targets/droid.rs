use super::shared::*;
use super::types::*;
use serde_json::json;
use std::path::PathBuf;
#[derive(Debug, Clone, Copy, Default)]
pub struct DroidTarget;
fn path(loc: Location) -> PathBuf {
    let base = if loc == Location::Local {
        std::env::current_dir().expect("working directory")
    } else {
        home_dir()
    };
    base.join(".factory/mcp.json")
}

fn entry() -> serde_json::Value {
    mcp_server_value()
}
impl AgentTarget for DroidTarget {
    fn id(&self) -> TargetId {
        TargetId::Droid
    }
    fn display_name(&self) -> &'static str {
        "Factory Droid"
    }
    fn docs_url(&self) -> Option<&'static str> {
        Some("https://docs.factory.com/cli/configuration/mcp")
    }
    fn supports_location(&self, _: Location) -> bool {
        true
    }
    fn detect(&self, loc: Location) -> DetectionResult {
        let base = if loc == Location::Global {
            home_dir()
        } else {
            std::env::current_dir().expect("working directory")
        };
        DetectionResult {
            installed: base.join(".factory").exists(),
            already_configured: read_json_file(path(loc))
                .pointer("/mcpServers/rustcodegraph")
                .is_some(),
            config_path: Some(path_to_string(path(loc))),
        }
    }
    fn install(&self, loc: Location, _: InstallOptions) -> WriteResult {
        WriteResult::with_file(
            crate::installer::config_writer::upsert_mcp_server(&path(loc), &entry())
                .expect("write Droid config"),
        )
    }
    fn uninstall(&self, loc: Location) -> WriteResult {
        let path = path(loc);
        let unchanged =
            || WriteResult::with_file(file_write(path_to_string(&path), WriteAction::NotFound));
        if !path.exists() {
            return unchanged();
        }
        let root = jsonc_parser::cst::CstRootNode::parse(&read_text(&path), &Default::default())
            .expect("invalid Droid JSONC");
        let Some(object) = root.object_value() else {
            return unchanged();
        };
        let Some(servers) = object.get("mcpServers").and_then(|p| p.object_value()) else {
            return unchanged();
        };
        let Some(server) = servers.get("rustcodegraph") else {
            return unchanged();
        };
        server.remove();
        if servers.properties().is_empty() {
            object.get("mcpServers").unwrap().remove();
        }
        super::shared::atomic_write_file_sync(&path, &root.to_string())
            .expect("write Droid config");
        WriteResult::with_file(file_write(path_to_string(path), WriteAction::Removed))
    }
    fn print_config(&self, loc: Location) -> String {
        format!(
            "# Add to {}\n\n{}\n",
            path_to_string(path(loc)),
            serde_json::to_string_pretty(&json!({"mcpServers":{"rustcodegraph":entry()}})).unwrap()
        )
    }
    fn describe_paths(&self, loc: Location) -> Vec<String> {
        vec![path_to_string(path(loc))]
    }
}
