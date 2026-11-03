use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use super::shared::{get_mcp_server_config, path_to_string, read_text};
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
pub struct GrokTarget;

fn config_dir(loc: Location) -> PathBuf {
    if loc == Location::Global {
        super::shared::config_dir_from_env("GROK_HOME", super::shared::home_dir().join(".grok"))
    } else {
        std::env::current_dir()
            .expect("working directory")
            .join(".grok")
    }
}
fn toml_config_path(loc: Location) -> PathBuf {
    config_dir(loc).join("config.toml")
}
fn configured(content: &str) -> bool {
    content
        .parse::<toml::Value>()
        .ok()
        .and_then(|v| {
            v.get("mcp_servers")
                .and_then(|v| v.get("rustcodegraph"))
                .cloned()
        })
        .is_some()
}

impl AgentTarget for GrokTarget {
    fn id(&self) -> TargetId {
        TargetId::Grok
    }

    fn display_name(&self) -> &'static str {
        "Grok Build"
    }

    fn docs_url(&self) -> Option<&'static str> {
        Some("https://docs.x.ai/build/features/mcp-servers")
    }

    fn supports_location(&self, _loc: Location) -> bool {
        true
    }

    fn detect(&self, loc: Location) -> DetectionResult {
        let toml_path = toml_config_path(loc);
        let content = read_text(&toml_path);
        DetectionResult {
            installed: config_dir(loc).exists(),
            already_configured: configured(&content),
            config_path: Some(path_to_string(toml_path)),
        }
    }

    fn install(&self, loc: Location, _opts: InstallOptions) -> WriteResult {
        let mut result = WriteResult::empty();
        result.push(write_mcp_entry(loc));
        result.notes.get_or_insert_with(Vec::new).push("Start a new Grok session, or press r in /mcps, to load the server. Existing or incompatible TOML is preserved; use print-config to inspect the desired entry.".into());
        if loc == Location::Local {
            result
                .notes
                .get_or_insert_with(Vec::new)
                .push("Grok applies project configuration only in a trusted folder.".into());
        }
        result
    }

    fn uninstall(&self, loc: Location) -> WriteResult {
        let mut result = WriteResult::empty();
        let path = toml_config_path(loc);
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
        result
    }

    fn print_config(&self, loc: Location) -> String {
        format!(
            "# Add to {}\n\n{}\n",
            path_to_string(toml_config_path(loc)),
            build_rustcodegraph_block()
        )
    }

    fn describe_paths(&self, loc: Location) -> Vec<String> {
        vec![path_to_string(toml_config_path(loc))]
    }
}

fn build_rustcodegraph_block() -> String {
    let mcp = get_mcp_server_config();
    let mut values = BTreeMap::new();
    values.insert("args".to_owned(), TomlValue::Strings(mcp.args));
    values.insert("command".to_owned(), TomlValue::String(mcp.command));
    build_toml_table(TOML_HEADER, values)
}

fn write_mcp_entry(loc: Location) -> FileWrite {
    let file = toml_config_path(loc);
    let existing = read_text(&file);
    if configured(&existing)
        || (!existing.trim().is_empty() && existing.parse::<toml::Value>().is_err())
    {
        return file_write(path_to_string(file), WriteAction::Unchanged);
    }
    let created = existing.is_empty() && fs::metadata(&file).is_err();
    // `upsert_toml_table` 不做完整 TOML round-trip，目的是最大限度保留
    // 用户注释和非 RustCodeGraph 表。
    let upsert = upsert_toml_table(&existing, TOML_HEADER, &build_rustcodegraph_block());
    if upsert.content.parse::<toml::Value>().is_err() {
        return file_write(path_to_string(file), WriteAction::Unchanged);
    }
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
