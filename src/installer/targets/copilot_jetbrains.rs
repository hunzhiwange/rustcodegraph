//! JetBrains Copilot user-level MCP configuration; upstream 15483713.
use super::shared::{home_dir, mcp_server_value, path_to_string, read_text};
use super::types::*;
use serde_json::{Value, json};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, Default)]
pub struct CopilotJetbrainsTarget;

fn user_dir() -> PathBuf {
    let fallback = if cfg!(windows) {
        super::shared::config_dir_from_env("LOCALAPPDATA", home_dir().join("AppData/Local"))
    } else {
        home_dir().join(".config")
    };
    super::shared::config_dir_from_env("XDG_CONFIG_HOME", fallback).join("github-copilot/intellij")
}
fn config_path(_: Location) -> PathBuf {
    user_dir().join("mcp.json")
}
fn entry(_: Location) -> Value {
    mcp_server_value()
}
impl AgentTarget for CopilotJetbrainsTarget {
    fn id(&self) -> TargetId {
        TargetId::CopilotJetbrains
    }
    fn display_name(&self) -> &'static str {
        "JetBrains IDEs (Copilot plugin)"
    }
    fn docs_url(&self) -> Option<&'static str> {
        Some(
            "https://docs.github.com/en/copilot/how-tos/provide-context/use-mcp/extend-copilot-chat-with-mcp",
        )
    }
    fn supports_location(&self, loc: Location) -> bool {
        loc == Location::Global
    }
    fn detect(&self, loc: Location) -> DetectionResult {
        if loc == Location::Local {
            return DetectionResult::default();
        }
        let path = config_path(loc);
        let config = jsonc_parser::parse_to_serde_value(&read_text(&path), &Default::default())
            .ok()
            .flatten();
        DetectionResult {
            installed: user_dir().exists()
                || home_dir()
                    .join("Library/Application Support/JetBrains")
                    .exists(),
            already_configured: config
                .as_ref()
                .and_then(|v| v.pointer("/servers/rustcodegraph"))
                .is_some(),
            config_path: Some(path_to_string(path)),
        }
    }
    fn install(&self, loc: Location, _: InstallOptions) -> WriteResult {
        if loc == Location::Local {
            return WriteResult::empty().with_notes([
                "Use --location=global; JetBrains Copilot has no project-local MCP configuration.",
            ]);
        }
        let path = config_path(loc);
        let file = crate::installer::config_writer::upsert_mcp_server_under(
            &path,
            "servers",
            &entry(loc),
            false,
        )
        .unwrap_or_else(|e| panic!("failed to write {}: {e}", path.display()));
        WriteResult::with_file(file)
            .with_notes(["Restart your JetBrains IDE for MCP changes to take effect."])
    }
    fn uninstall(&self, loc: Location) -> WriteResult {
        if loc == Location::Local {
            return WriteResult::empty().with_notes([
                "Use --location=global; JetBrains Copilot has no project-local MCP configuration.",
            ]);
        }
        let path = config_path(loc);
        let unchanged =
            || WriteResult::with_file(file_write(path_to_string(&path), WriteAction::NotFound));
        if !path.exists() {
            return unchanged();
        }
        let root = jsonc_parser::cst::CstRootNode::parse(&read_text(&path), &Default::default())
            .expect("invalid JetBrains Copilot JSONC");
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
            .expect("write JetBrains Copilot config");
        WriteResult::with_file(file_write(path_to_string(path), WriteAction::Removed))
    }
    fn print_config(&self, loc: Location) -> String {
        if loc == Location::Local {
            return "# Use --location=global.\n".into();
        }
        format!(
            "# Add to {}\n\n{}\n",
            path_to_string(config_path(loc)),
            serde_json::to_string_pretty(&json!({"servers": {"rustcodegraph": entry(loc)}}))
                .unwrap()
        )
    }
    fn describe_paths(&self, loc: Location) -> Vec<String> {
        if loc == Location::Local {
            return vec![];
        }
        vec![path_to_string(config_path(loc))]
    }
}
