use super::shared::*;
use super::types::*;
use serde_json::json;
use std::path::PathBuf;
#[derive(Debug, Clone, Copy, Default)]
pub struct QwenTarget;
fn path(loc: Location) -> PathBuf {
    let dir = if loc == Location::Global {
        config_dir_from_env("QWEN_HOME", home_dir().join(".qwen"))
    } else {
        current_dir().join(".qwen")
    };
    dir.join("settings.json")
}
fn entry() -> serde_json::Value {
    let mut v = mcp_server_value();
    v.as_object_mut().unwrap().remove("type");
    v
}
impl AgentTarget for QwenTarget {
    fn id(&self) -> TargetId {
        TargetId::Qwen
    }
    fn display_name(&self) -> &'static str {
        "Qwen Code"
    }
    fn docs_url(&self) -> Option<&'static str> {
        Some("https://qwenlm.github.io/qwen-code-docs/en/users/features/mcp/")
    }
    fn supports_location(&self, _: Location) -> bool {
        true
    }
    fn detect(&self, loc: Location) -> DetectionResult {
        DetectionResult {
            installed: path(loc).parent().unwrap().exists(),
            already_configured: jsonc_parser::parse_to_serde_value(
                &read_text(path(loc)),
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
    fn install(&self, loc: Location, opts: InstallOptions) -> WriteResult {
        let p = path(loc);
        let mut file = crate::installer::config_writer::upsert_mcp_server(&p, &entry())
            .expect("write Qwen config");
        if opts.auto_allow {
            use jsonc_parser::cst::{CstInputValue, CstRootNode};
            let before = read_text(&p);
            let root = CstRootNode::parse(&before, &Default::default()).expect("Qwen JSONC");
            let object = root.object_value().unwrap();
            if let Some(permissions) = object.object_value_or_create("permissions") {
                let value = jsonc_parser::parse_to_serde_value(&before, &Default::default())
                    .unwrap()
                    .unwrap();
                let allow = value.pointer("/permissions/allow");
                if allow.is_none()
                    || allow.is_some_and(|v| {
                        v.as_array()
                            .is_some_and(|a| a.iter().all(|v| v.is_string()))
                    })
                {
                    let mut rules = allow
                        .and_then(|v| v.as_array())
                        .cloned()
                        .unwrap_or_default();
                    if !rules.iter().any(|v| v == "mcp__rustcodegraph") {
                        rules.push(serde_json::json!("mcp__rustcodegraph"));
                        if let Some(old) = permissions.get("allow") {
                            old.remove();
                        }
                        permissions.append(
                            "allow",
                            CstInputValue::Array(
                                rules
                                    .iter()
                                    .map(|v| CstInputValue::String(v.as_str().unwrap().into()))
                                    .collect(),
                            ),
                        );
                        atomic_write_file_sync(&p, &root.to_string())
                            .expect("write Qwen permissions");
                        if file.action == WriteAction::Unchanged {
                            file.action = WriteAction::Updated;
                        }
                    }
                }
            }
        }
        WriteResult::with_file(file)
    }
    fn uninstall(&self, loc: Location) -> WriteResult {
        let path = path(loc);
        let unchanged =
            || WriteResult::with_file(file_write(path_to_string(&path), WriteAction::NotFound));
        if !path.exists() {
            return unchanged();
        }
        let root = jsonc_parser::cst::CstRootNode::parse(&read_text(&path), &Default::default())
            .expect("invalid Qwen JSONC");
        let Some(object) = root.object_value() else {
            return unchanged();
        };
        let mut changed = false;
        if let Some(servers) = object.get("mcpServers").and_then(|p| p.object_value())
            && let Some(server) = servers.get("rustcodegraph")
        {
            server.remove();
            changed = true;
            if servers.properties().is_empty() {
                object.get("mcpServers").unwrap().remove();
            }
        }
        if let Some(permissions) = object.get("permissions").and_then(|p| p.object_value())
            && let Some(allow) = permissions.array_value("allow")
        {
            let mut removed_rule = false;
            for rule in allow.elements() {
                if rule.to_string() == "\"mcp__rustcodegraph\"" {
                    rule.remove();
                    removed_rule = true;
                    changed = true;
                }
            }
            if removed_rule && allow.elements().is_empty() {
                permissions.get("allow").unwrap().remove();
                if permissions.properties().is_empty() {
                    object.get("permissions").unwrap().remove();
                }
            }
        }
        if !changed {
            return unchanged();
        }
        super::shared::atomic_write_file_sync(&path, &root.to_string()).expect("write Qwen config");
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
