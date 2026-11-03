use super::shared::*;
use super::types::*;
use serde_json::json;
use std::path::PathBuf;
#[derive(Debug, Clone, Copy, Default)]
pub struct BobTarget;
fn path(loc: Location) -> PathBuf {
    if loc == Location::Local {
        return std::env::current_dir()
            .expect("working directory")
            .join(".bob/mcp.json");
    }
    let dir = home_dir().join(".bob/settings");
    let current = dir.join("mcp.json");
    let earlier = dir.join("mcp_settings.json");
    if !current.exists() && earlier.exists() {
        earlier
    } else {
        current
    }
}
fn entry() -> serde_json::Value {
    let mut v = mcp_server_value();
    v.as_object_mut().unwrap().remove("type");
    v
}
impl AgentTarget for BobTarget {
    fn id(&self) -> TargetId {
        TargetId::Bob
    }
    fn display_name(&self) -> &'static str {
        "IBM Bob"
    }
    fn docs_url(&self) -> Option<&'static str> {
        Some("https://bob.ibm.com/docs/ide/configuration/mcp/mcp-in-bob")
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
            installed: base.join(".bob").exists(),
            already_configured: read_json_file(path(loc))
                .pointer("/mcpServers/rustcodegraph")
                .is_some(),
            config_path: Some(path_to_string(path(loc))),
        }
    }
    fn install(&self, loc: Location, _: InstallOptions) -> WriteResult {
        WriteResult::with_file(
            crate::installer::config_writer::upsert_mcp_server(&path(loc), &entry())
                .expect("write Bob config"),
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
            .expect("invalid Bob JSONC");
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
        super::shared::atomic_write_file_sync(&path, &root.to_string()).expect("write Bob config");
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
