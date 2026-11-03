use super::shared::*;
use super::types::*;
use serde_json::json;
use std::path::PathBuf;
#[derive(Debug, Clone, Copy, Default)]
pub struct QoderTarget;
fn path(loc: Location) -> PathBuf {
    let dir = if loc == Location::Global {
        config_dir_from_env("QODER_CONFIG_DIR", home_dir().join(".qoder"))
    } else {
        current_dir().join(".qoder")
    };
    dir.join(if loc == Location::Global {
        "settings.json"
    } else {
        "settings.local.json"
    })
}
fn entry() -> serde_json::Value {
    let mut v = mcp_server_value();
    v.as_object_mut().unwrap().remove("type");
    v
}
impl AgentTarget for QoderTarget {
    fn id(&self) -> TargetId {
        TargetId::Qoder
    }
    fn display_name(&self) -> &'static str {
        "Qoder CLI"
    }
    fn docs_url(&self) -> Option<&'static str> {
        Some("https://docs.qoder.com/cli/mcp-reference")
    }
    fn supports_location(&self, _: Location) -> bool {
        true
    }
    fn detect(&self, loc: Location) -> DetectionResult {
        DetectionResult {
            installed: path(loc).parent().unwrap().exists(),
            already_configured: jsonc_parser::parse_to_serde_value(
                &read_text(&path(loc)),
                &Default::default(),
            )
            .ok()
            .flatten()
            .unwrap_or_default()
            .pointer("/mcpServers/rustcodegraph")
            .is_some(),
            config_path: Some(path_to_string(path(loc))),
        }
    }
    fn install(&self, loc: Location, _: InstallOptions) -> WriteResult {
        let mut result = WriteResult::with_file(
            crate::installer::config_writer::upsert_mcp_server(&path(loc), &entry())
                .expect("write Qoder config"),
        );
        let mut notes = vec![
            "This sets up the Qoder CLI. In the Qoder IDE, add the server under Settings > MCP."
                .into(),
        ];
        if loc == Location::Local {
            notes.push("Qoder CLI loads project MCP servers only in a trusted folder.".into());
        }
        result.notes = Some(notes);
        result
    }
    fn uninstall(&self, loc: Location) -> WriteResult {
        let path = path(loc);
        let unchanged =
            || WriteResult::with_file(file_write(path_to_string(&path), WriteAction::NotFound));
        if !path.exists() {
            return unchanged();
        }
        let root = jsonc_parser::cst::CstRootNode::parse(&read_text(&path), &Default::default())
            .expect("invalid Qoder JSONC");
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
            .expect("write Qoder config");
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
