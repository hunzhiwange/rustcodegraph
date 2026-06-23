//! OpenAI Codex CLI installer target.
//!
//! Codex 使用 TOML 配置，并且用户文件里可能有其它 sibling table 或
//! array-of-tables。本 target 只替换 `[mcp_servers.rustcodegraph]`，把
//! 其它 TOML 原样留给窄口 serializer 处理。

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use super::shared::{
    get_mcp_server_config, path_to_string, read_text, remove_rustcodegraph_instructions,
    upsert_instructions_entry,
};
use super::toml::{
    RemoveTomlAction, TomlValue, UpsertTomlAction, build_toml_table, remove_toml_table,
    upsert_toml_table,
};
use super::types::{
    AgentTarget, DetectionResult, FileWrite, InstallOptions, Location, TargetId, WriteAction,
    WriteResult, file_write,
};

const TOML_HEADER: &str = "mcp_servers.rustcodegraph";

#[derive(Debug, Clone, Copy, Default)]
pub struct CodexTarget;

fn config_dir() -> PathBuf {
    super::shared::home_dir().join(".codex")
}

fn toml_config_path() -> PathBuf {
    config_dir().join("config.toml")
}

fn instructions_path() -> PathBuf {
    config_dir().join("AGENTS.md")
}

impl AgentTarget for CodexTarget {
    fn id(&self) -> TargetId {
        TargetId::Codex
    }

    fn display_name(&self) -> &'static str {
        "Codex CLI"
    }

    fn docs_url(&self) -> Option<&'static str> {
        Some("https://github.com/openai/codex")
    }

    fn supports_location(&self, loc: Location) -> bool {
        loc == Location::Global
    }

    fn detect(&self, loc: Location) -> DetectionResult {
        if loc != Location::Global {
            return DetectionResult::default();
        }
        let toml_path = toml_config_path();
        let content = read_text(&toml_path);
        DetectionResult {
            installed: config_dir().exists(),
            already_configured: content.contains(&format!("[{TOML_HEADER}]")),
            config_path: Some(path_to_string(toml_path)),
        }
    }

    fn install(&self, loc: Location, _opts: InstallOptions) -> WriteResult {
        if loc != Location::Global {
            return WriteResult::empty().with_notes([
                "Codex CLI has no project-local config - re-run with --location=global to install.",
            ]);
        }
        let mut result = WriteResult::empty();
        result.push(write_mcp_entry());
        result.push(upsert_instructions_entry(instructions_path()));
        result
    }

    fn uninstall(&self, loc: Location) -> WriteResult {
        if loc != Location::Global {
            return WriteResult::empty();
        }
        let mut result = WriteResult::empty();
        let path = toml_config_path();
        if path.exists() {
            let removed = remove_toml_table(&read_text(&path), TOML_HEADER);
            let action = match removed.action {
                RemoveTomlAction::Removed => {
                    let _ = super::shared::atomic_write_file_sync(&path, &removed.content);
                    WriteAction::Removed
                }
                RemoveTomlAction::NotFound => WriteAction::NotFound,
            };
            result.push(file_write(path_to_string(path), action));
        } else {
            result.push(file_write(path_to_string(path), WriteAction::NotFound));
        }
        result.push(remove_instructions_entry());
        result
    }

    fn print_config(&self, loc: Location) -> String {
        if loc != Location::Global {
            return "# Codex CLI has no project-local config - use --location=global.\n".to_owned();
        }
        format!(
            "# Add to {}\n\n{}\n",
            path_to_string(toml_config_path()),
            build_rustcodegraph_block()
        )
    }

    fn describe_paths(&self, loc: Location) -> Vec<String> {
        if loc != Location::Global {
            Vec::new()
        } else {
            vec![
                path_to_string(toml_config_path()),
                path_to_string(instructions_path()),
            ]
        }
    }
}

fn build_rustcodegraph_block() -> String {
    let mcp = get_mcp_server_config();
    let mut values = BTreeMap::new();
    values.insert("args".to_owned(), TomlValue::Strings(mcp.args));
    values.insert("command".to_owned(), TomlValue::String(mcp.command));
    build_toml_table(TOML_HEADER, values)
}

fn write_mcp_entry() -> FileWrite {
    let file = toml_config_path();
    let existing = read_text(&file);
    let created = existing.is_empty() && fs::metadata(&file).is_err();
    // `upsert_toml_table` 不做完整 TOML round-trip，目的是最大限度保留
    // 用户注释和非 RustCodeGraph 表。
    let upsert = upsert_toml_table(&existing, TOML_HEADER, &build_rustcodegraph_block());
    let action = match upsert.action {
        UpsertTomlAction::Unchanged => WriteAction::Unchanged,
        UpsertTomlAction::Inserted | UpsertTomlAction::Replaced => {
            if created {
                WriteAction::Created
            } else {
                WriteAction::Updated
            }
        }
    };
    if action != WriteAction::Unchanged {
        let _ = super::shared::atomic_write_file_sync(&file, &upsert.content);
    }
    file_write(path_to_string(file), action)
}

fn remove_instructions_entry() -> FileWrite {
    remove_rustcodegraph_instructions(instructions_path())
}
