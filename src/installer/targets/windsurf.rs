use super::shared::*;
use super::types::*;
use serde_json::json;
use std::path::PathBuf;
#[derive(Debug, Clone, Copy, Default)]
pub struct WindsurfTarget;
fn path(loc: Location) -> PathBuf {
    let _ = loc;
    home_dir().join(".codeium/windsurf/mcp_config.json")
}

fn entry() -> serde_json::Value {
    let mut value = mcp_server_value();
    value.as_object_mut().unwrap().remove("type");
    value
}
impl AgentTarget for WindsurfTarget {
    fn id(&self) -> TargetId {
        TargetId::Windsurf
    }
    fn display_name(&self) -> &'static str {
        "Windsurf"
    }
    fn docs_url(&self) -> Option<&'static str> {
        Some("https://docs.codeium/windsurf.com/cli/configuration/mcp")
    }
    fn supports_location(&self, loc: Location) -> bool {
        loc == Location::Global
    }
    fn detect(&self, loc: Location) -> DetectionResult {
        if loc == Location::Local {
            return DetectionResult::default();
        }
        let base = if loc == Location::Global {
            home_dir()
        } else {
            std::env::current_dir().expect("working directory")
        };
        DetectionResult {
            installed: base.join(".codeium/windsurf").exists(),
            already_configured: read_json_file(path(loc))
                .pointer("/mcpServers/rustcodegraph")
                .is_some(),
            config_path: Some(path_to_string(path(loc))),
        }
    }
    fn install(&self, loc: Location, _: InstallOptions) -> WriteResult {
        if loc == Location::Local {
            return WriteResult::empty().with_notes(["Windsurf requires --location=global"]);
        }
        WriteResult::with_file(
            crate::installer::config_writer::upsert_mcp_server(&path(loc), &entry())
                .expect("write Windsurf config"),
        )
    }
    fn uninstall(&self, loc: Location) -> WriteResult {
        if loc == Location::Local {
            return WriteResult::empty().with_notes(["Windsurf requires --location=global"]);
        }
        let path = path(loc);
        let unchanged =
            || WriteResult::with_file(file_write(path_to_string(&path), WriteAction::NotFound));
        if !path.exists() {
            return unchanged();
        }
        let root = jsonc_parser::cst::CstRootNode::parse(&read_text(&path), &Default::default())
            .expect("invalid Windsurf JSONC");
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
            .expect("write Windsurf config");
        WriteResult::with_file(file_write(path_to_string(path), WriteAction::Removed))
    }
    fn print_config(&self, loc: Location) -> String {
        if loc == Location::Local {
            return "# Windsurf requires --location=global\n".into();
        }
        format!(
            "# Add to {}\n\n{}\n",
            path_to_string(path(loc)),
            serde_json::to_string_pretty(&json!({"mcpServers":{"rustcodegraph":entry()}})).unwrap()
        )
    }
    fn describe_paths(&self, loc: Location) -> Vec<String> {
        if loc == Location::Local {
            return vec![];
        }
        vec![path_to_string(path(loc))]
    }
}
