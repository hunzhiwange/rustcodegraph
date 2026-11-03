//! Copilot CLI compatibility target, fixed upstream 15483713 global-only scope.
//! Newer CLI project configuration is outside this target's installation scope.
use super::shared::*;
use super::types::*;
use serde_json::{Value, json};
use std::path::PathBuf;
#[derive(Debug, Clone, Copy, Default)]
pub struct CopilotCliTarget;
fn dir() -> PathBuf {
    config_dir_from_env("COPILOT_HOME", home_dir().join(".copilot"))
}
fn path() -> PathBuf {
    dir().join("mcp-config.json")
}
fn entry() -> Value {
    let mut v = mcp_server_value();
    v["tools"] = json!(["*"]);
    v["deferTools"] = json!("never");
    v
}
fn unsupported() -> WriteResult {
    WriteResult::default().with_notes([
        "Copilot CLI target supports global configuration only; use --location=global.",
    ])
}
impl AgentTarget for CopilotCliTarget {
    fn id(&self) -> TargetId {
        TargetId::CopilotCli
    }
    fn display_name(&self) -> &'static str {
        "GitHub Copilot CLI"
    }
    fn docs_url(&self) -> Option<&'static str> {
        Some(
            "https://docs.github.com/en/copilot/how-tos/copilot-cli/customize-copilot/add-mcp-servers",
        )
    }
    fn supports_location(&self, loc: Location) -> bool {
        loc == Location::Global
    }
    fn detect(&self, loc: Location) -> DetectionResult {
        if loc != Location::Global {
            return DetectionResult::default();
        }
        let footprint = std::fs::read_dir(dir())
            .map(|entries| entries.flatten().any(|e| e.file_name() != "ide"))
            .unwrap_or(false);
        let on_path = std::env::var_os("PATH")
            .map(|p| {
                std::env::split_paths(&p).any(|d| {
                    let names: &[&str] = if cfg!(windows) {
                        &["copilot.exe", "copilot.cmd", "copilot.bat", "copilot.ps1"]
                    } else {
                        &["copilot"]
                    };
                    names.iter().any(|name| d.join(name).exists())
                })
            })
            .unwrap_or(false);
        DetectionResult {
            installed: footprint || on_path,
            already_configured: read_json_file(path())
                .pointer("/mcpServers/rustcodegraph")
                .is_some(),
            config_path: Some(path_to_string(path())),
        }
    }
    fn install(&self, loc: Location, _: InstallOptions) -> WriteResult {
        if loc != Location::Global {
            return unsupported();
        }
        WriteResult::with_file(
            crate::installer::config_writer::upsert_mcp_server(&path(), &entry())
                .expect("write Copilot CLI config"),
        )
    }
    fn uninstall(&self, loc: Location) -> WriteResult {
        if loc != Location::Global {
            return unsupported();
        }
        WriteResult::with_file(planned_json_mcp_remove(path()))
    }
    fn print_config(&self, loc: Location) -> String {
        if loc != Location::Global {
            return "# Copilot CLI target supports global configuration only; use --location=global.\n".into();
        }
        format!(
            "# Add to {}\n\n{}\n",
            path_to_string(path()),
            serde_json::to_string_pretty(&json!({"mcpServers":{"rustcodegraph":entry()}})).unwrap()
        )
    }
    fn describe_paths(&self, loc: Location) -> Vec<String> {
        if loc == Location::Global {
            vec![path_to_string(path())]
        } else {
            vec![]
        }
    }
}
