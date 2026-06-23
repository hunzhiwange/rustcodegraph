//! Google Antigravity IDE installer target.
//!
//! Antigravity 迁移过一次配置目录：新路径在 `.gemini/config`，旧路径在
//! `.gemini/antigravity`。安装时写首选路径并清理旧条目，避免同一个 MCP
//! server 在 IDE 中出现两份。

use std::path::PathBuf;

use serde_json::json;

use super::shared::{
    mcp_json_snippet, path_to_string, planned_json_mcp_remove, planned_json_mcp_write,
    read_json_file, write_json_file,
};
use super::types::{
    AgentTarget, DetectionResult, FileWrite, InstallOptions, Location, TargetId, WriteAction,
    WriteResult,
};

#[derive(Debug, Clone, Copy, Default)]
pub struct AntigravityTarget;

fn unified_config_dir() -> PathBuf {
    super::shared::home_dir().join(".gemini").join("config")
}

fn unified_mcp_config_path() -> PathBuf {
    unified_config_dir().join("mcp_config.json")
}

fn legacy_config_dir() -> PathBuf {
    super::shared::home_dir()
        .join(".gemini")
        .join("antigravity")
}

fn legacy_mcp_config_path() -> PathBuf {
    legacy_config_dir().join("mcp_config.json")
}

fn migrated_marker_path() -> PathBuf {
    unified_config_dir().join(".migrated")
}

fn preferred_mcp_config_path() -> PathBuf {
    // 有迁移标记或新文件时偏向统一配置目录；否则尊重仍在使用旧目录的安装。
    if migrated_marker_path().exists() || unified_mcp_config_path().exists() {
        unified_mcp_config_path()
    } else {
        legacy_mcp_config_path()
    }
}

fn resolve_rustcodegraph_command() -> String {
    // The TypeScript installer runs `command -v` on macOS so GUI launches can
    // find nvm-managed binaries. This Rust translation records the decision
    // point but deliberately does not execute shell commands yet.
    "rustcodegraph".to_owned()
}

fn build_antigravity_entry() -> serde_json::Value {
    json!({
        "command": resolve_rustcodegraph_command(),
        "args": ["serve", "--mcp"]
    })
}

impl AgentTarget for AntigravityTarget {
    fn id(&self) -> TargetId {
        TargetId::Antigravity
    }

    fn display_name(&self) -> &'static str {
        "Antigravity IDE"
    }

    fn docs_url(&self) -> Option<&'static str> {
        Some("https://antigravity.google")
    }

    fn supports_location(&self, loc: Location) -> bool {
        loc == Location::Global
    }

    fn detect(&self, loc: Location) -> DetectionResult {
        if loc != Location::Global {
            return DetectionResult::default();
        }
        let file = preferred_mcp_config_path();
        let config = read_json_file(&file);
        DetectionResult {
            installed: unified_config_dir().exists()
                || legacy_config_dir().exists()
                || file.exists(),
            already_configured: config.pointer("/mcpServers/rustcodegraph").is_some(),
            config_path: Some(path_to_string(file)),
        }
    }

    fn install(&self, loc: Location, _opts: InstallOptions) -> WriteResult {
        if loc != Location::Global {
            return WriteResult::empty().with_notes([
                "Antigravity IDE has no project-local config - re-run with --location=global.",
            ]);
        }
        let mut result = WriteResult::empty();
        result.push(write_mcp_entry());
        if let Some(cleanup) = cleanup_legacy_entry() {
            result.push(cleanup);
        }
        result.with_notes(["Restart Antigravity for MCP changes to take effect."])
    }

    fn uninstall(&self, loc: Location) -> WriteResult {
        if loc != Location::Global {
            return WriteResult::empty();
        }
        let mut result = WriteResult::empty();
        let preferred = preferred_mcp_config_path();
        result.push(remove_rustcodegraph_from_file(&preferred));
        let legacy = legacy_mcp_config_path();
        let unified = unified_mcp_config_path();
        let alternate = if preferred == legacy { unified } else { legacy };
        let cleanup = remove_rustcodegraph_from_file(&alternate);
        if cleanup.action == WriteAction::Removed {
            result.push(cleanup);
        }
        result
    }

    fn print_config(&self, loc: Location) -> String {
        if loc != Location::Global {
            return "# Antigravity IDE has no project-local config - use --location=global.\n"
                .to_owned();
        }
        format!(
            "# Add to {}\n\n{}\n",
            path_to_string(preferred_mcp_config_path()),
            mcp_json_snippet(build_antigravity_entry())
        )
    }

    fn describe_paths(&self, loc: Location) -> Vec<String> {
        if loc != Location::Global {
            Vec::new()
        } else {
            vec![path_to_string(preferred_mcp_config_path())]
        }
    }
}

fn write_mcp_entry() -> FileWrite {
    planned_json_mcp_write(preferred_mcp_config_path(), build_antigravity_entry())
}

fn cleanup_legacy_entry() -> Option<FileWrite> {
    // 只在首选路径已经切到新目录时清理 legacy 文件，避免把仍在旧目录上
    // 工作的安装状态误删。
    let preferred = preferred_mcp_config_path();
    let legacy = legacy_mcp_config_path();
    if preferred == legacy || !legacy.exists() {
        return None;
    }
    let cleanup = remove_rustcodegraph_from_file(&legacy);
    (cleanup.action == WriteAction::Removed).then_some(cleanup)
}

fn remove_rustcodegraph_from_file(path: impl AsRef<std::path::Path>) -> FileWrite {
    let path = path.as_ref();
    if !path.exists() {
        return super::types::file_write(path_to_string(path), WriteAction::NotFound);
    }
    let mut config = read_json_file(path);
    let mut removed = false;
    if let Some(servers) = config
        .get_mut("mcpServers")
        .and_then(|value| value.as_object_mut())
    {
        removed = servers.remove("rustcodegraph").is_some();
        if servers.is_empty() {
            config
                .as_object_mut()
                .and_then(|root| root.remove("mcpServers"));
        }
    }
    if removed {
        let _ = write_json_file(path, &config);
        super::types::file_write(path_to_string(path), WriteAction::Removed)
    } else {
        planned_json_mcp_remove(path)
    }
}
