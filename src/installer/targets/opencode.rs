//! opencode installer target.
//!
//! opencode 支持 JSONC，用户配置常带注释；纯 JSON 走 serde_json，含注释时
//! 使用本文件的窄范围扫描器，只插入/删除 `mcp.rustcodegraph`。

use std::env;
use std::path::{Path, PathBuf};

use serde_json::json;

use super::shared::{
    atomic_write_file_sync, current_dir, json_deep_equal, path_to_string, read_json_file,
    read_text, remove_rustcodegraph_instructions, upsert_instructions_entry, write_json_file,
};
use super::types::{
    AgentTarget, DetectionResult, FileWrite, InstallOptions, Location, TargetId, WriteAction,
    WriteResult, file_write,
};

#[derive(Debug, Clone, Copy, Default)]
pub struct OpencodeTarget;

fn global_config_dir() -> PathBuf {
    let base = env::var_os("XDG_CONFIG_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| super::shared::home_dir().join(".config"));
    base.join("opencode")
}

fn legacy_windows_config_dir() -> Option<PathBuf> {
    // Windows 旧版使用 `%APPDATA%/opencode`，新版随 XDG/global_config_dir。
    // 若两者解析到同一路径，就不把它当 legacy 清理。
    let app_data = env::var_os("APPDATA").filter(|value| !value.is_empty())?;
    let legacy = PathBuf::from(app_data).join("opencode");
    if legacy == global_config_dir() {
        None
    } else {
        Some(legacy)
    }
}

fn config_base_dir(loc: Location) -> PathBuf {
    match loc {
        Location::Global => global_config_dir(),
        Location::Local => current_dir(),
    }
}

fn config_path(loc: Location) -> PathBuf {
    // 已有 `.jsonc` 优先，其次沿用 `.json`；绿地安装创建 `.jsonc`，
    // 这样后续可以保留用户注释。
    let dir = config_base_dir(loc);
    let jsonc = dir.join("opencode.jsonc");
    let json = dir.join("opencode.json");
    if jsonc.exists() {
        jsonc
    } else if json.exists() {
        json
    } else {
        jsonc
    }
}

fn instructions_path(loc: Location) -> PathBuf {
    config_base_dir(loc).join("AGENTS.md")
}

fn get_opencode_server_entry() -> serde_json::Value {
    json!({
        "type": "local",
        "command": ["rustcodegraph", "serve", "--mcp"],
        "enabled": true
    })
}

impl AgentTarget for OpencodeTarget {
    fn id(&self) -> TargetId {
        TargetId::Opencode
    }

    fn display_name(&self) -> &'static str {
        "opencode"
    }

    fn docs_url(&self) -> Option<&'static str> {
        Some("https://opencode.ai/docs/config")
    }

    fn supports_location(&self, _loc: Location) -> bool {
        true
    }

    fn detect(&self, loc: Location) -> DetectionResult {
        let file = config_path(loc);
        let text = read_text(&file);
        let config = read_json_file(&file);
        let legacy = legacy_windows_config_dir();
        let installed = match loc {
            Location::Global => {
                global_config_dir().exists() || legacy.as_ref().is_some_and(|dir| dir.exists())
            }
            Location::Local => file.exists(),
        };
        DetectionResult {
            installed,
            already_configured: config.pointer("/mcp/rustcodegraph").is_some()
                || jsonc_has_mcp_entry(&text, "rustcodegraph"),
            config_path: Some(path_to_string(file)),
        }
    }

    fn install(&self, loc: Location, _opts: InstallOptions) -> WriteResult {
        let mut result = WriteResult::empty();
        result.push(write_mcp_entry(loc));
        result.push(upsert_instructions_entry(instructions_path(loc)));
        if loc == Location::Global {
            result.files.extend(cleanup_legacy_windows_state());
        }
        result
    }

    fn uninstall(&self, loc: Location) -> WriteResult {
        let mut result = WriteResult::empty();
        result.push(remove_mcp_entry_at(config_path(loc)));
        result.push(remove_instructions_entry(loc));
        if loc == Location::Global {
            result.files.extend(cleanup_legacy_windows_state());
        }
        result
    }

    fn print_config(&self, loc: Location) -> String {
        let snippet = serde_json::to_string_pretty(&json!({
            "$schema": "https://opencode.ai/config.json",
            "mcp": { "rustcodegraph": get_opencode_server_entry() }
        }))
        .unwrap_or_else(|_| "{}".to_owned());
        format!(
            "# Add to {}\n\n{}\n",
            path_to_string(config_path(loc)),
            snippet
        )
    }

    fn describe_paths(&self, loc: Location) -> Vec<String> {
        vec![
            path_to_string(config_path(loc)),
            path_to_string(instructions_path(loc)),
        ]
    }
}

fn write_mcp_entry(loc: Location) -> FileWrite {
    let file = config_path(loc);
    let existed = file.exists();
    let mut text = read_text(&file);
    if text.trim().is_empty() {
        text = "{\n  \"$schema\": \"https://opencode.ai/config.json\"\n}\n".to_owned();
    }
    let config = read_json_file(&file);
    let before = config.pointer("/mcp/rustcodegraph");
    if before.is_some_and(|value| json_deep_equal(value, &get_opencode_server_entry()))
        || (before.is_none() && jsonc_has_mcp_entry(&text, "rustcodegraph"))
    {
        return file_write(path_to_string(file), WriteAction::Unchanged);
    }

    let action = if existed {
        WriteAction::Updated
    } else {
        WriteAction::Created
    };
    if is_plain_json(&text) {
        let mut root = read_json_file(&file);
        if !root.is_object() {
            root = json!({});
        }
        let obj = root.as_object_mut().expect("opencode root is an object");
        obj.entry("$schema".to_owned())
            .or_insert_with(|| json!("https://opencode.ai/config.json"));
        let mcp = obj.entry("mcp".to_owned()).or_insert_with(|| json!({}));
        if !mcp.is_object() {
            *mcp = json!({});
        }
        mcp.as_object_mut()
            .expect("opencode mcp is an object")
            .insert("rustcodegraph".to_owned(), get_opencode_server_entry());
        let _ = write_json_file(&file, &root);
    } else {
        let updated = jsonc_upsert_mcp_entry(&text);
        let _ = atomic_write_file_sync(&file, &updated);
    }
    file_write(path_to_string(file), action)
}

fn remove_mcp_entry_at(path: impl AsRef<Path>) -> FileWrite {
    let path = path.as_ref();
    if !path.exists() {
        return file_write(path_to_string(path), WriteAction::NotFound);
    }
    let text = read_text(path);
    if is_plain_json(&text) {
        let mut config = read_json_file(path);
        let mut removed = false;
        if let Some(mcp) = config
            .get_mut("mcp")
            .and_then(|value| value.as_object_mut())
        {
            removed = mcp.remove("rustcodegraph").is_some();
            if mcp.is_empty() {
                config.as_object_mut().and_then(|root| root.remove("mcp"));
            }
        }
        if removed {
            let _ = write_json_file(path, &config);
            return file_write(path_to_string(path), WriteAction::Removed);
        }
        return file_write(path_to_string(path), WriteAction::NotFound);
    }

    let Some(updated) = jsonc_remove_mcp_entry(&text) else {
        return file_write(path_to_string(path), WriteAction::NotFound);
    };
    let _ = atomic_write_file_sync(path, &updated);
    file_write(path_to_string(path), WriteAction::Removed)
}

fn is_plain_json(text: &str) -> bool {
    // 足够保守即可：发现注释 token 就走 JSONC 手术路径，避免 serde_json 丢注释。
    !text.contains("//") && !text.contains("/*")
}

fn jsonc_has_mcp_entry(text: &str, key: &str) -> bool {
    let Some((mcp_start, mcp_end, _range)) = find_object_property(text, "mcp", 0, text.len())
    else {
        return false;
    };
    find_object_property(text, key, mcp_start, mcp_end).is_some()
}

fn jsonc_upsert_mcp_entry(text: &str) -> String {
    let mut text = if text.trim().is_empty() {
        "{\n  \"$schema\": \"https://opencode.ai/config.json\"\n}\n".to_owned()
    } else {
        text.to_owned()
    };
    if !text.contains("\"$schema\"") {
        text = insert_root_property(&text, "\"$schema\": \"https://opencode.ai/config.json\"");
    }
    if let Some((mcp_start, mcp_end, _range)) = find_object_property(&text, "mcp", 0, text.len()) {
        insert_object_property(
            &text,
            mcp_start,
            mcp_end,
            &format!("\"rustcodegraph\": {}", opencode_entry_json()),
        )
    } else {
        insert_root_property(
            &text,
            &format!(
                "\"mcp\": {{\n    \"rustcodegraph\": {}\n  }}",
                opencode_entry_json()
            ),
        )
    }
}

fn jsonc_remove_mcp_entry(text: &str) -> Option<String> {
    let (mcp_start, mcp_end, mcp_range) = find_object_property(text, "mcp", 0, text.len())?;
    let property = find_object_property(text, "rustcodegraph", mcp_start, mcp_end)?;
    let mut updated = remove_range_with_comma(text, property.2);
    if let Some((new_mcp_start, new_mcp_end, new_mcp_range)) =
        find_object_property(&updated, "mcp", 0, updated.len())
    {
        let inner = strip_jsonc_comments(&updated[new_mcp_start + 1..new_mcp_end]);
        if !inner.contains('"') {
            updated = remove_range_with_comma(&updated, new_mcp_range);
        }
    } else {
        let _ = mcp_range;
    }
    Some(updated)
}

fn opencode_entry_json() -> String {
    serde_json::to_string_pretty(&get_opencode_server_entry())
        .unwrap_or_else(|_| {
            "{\"type\":\"local\",\"command\":[\"rustcodegraph\",\"serve\",\"--mcp\"],\"enabled\":true}"
                .to_owned()
        })
        .replace('\n', "\n    ")
}

fn insert_root_property(text: &str, property: &str) -> String {
    let Some(root_start) = text.find('{') else {
        return format!("{{\n  {property}\n}}\n");
    };
    let Some(root_end) = find_matching_brace(text, root_start) else {
        return format!("{{\n  {property}\n}}\n");
    };
    insert_object_property(text, root_start, root_end, property)
}

fn insert_object_property(
    text: &str,
    object_start: usize,
    object_end: usize,
    property: &str,
) -> String {
    let inner = strip_jsonc_comments(&text[object_start + 1..object_end]);
    let comma = if inner.trim().is_empty() { "" } else { "," };
    let indent = if property.contains('\n') {
        "  "
    } else {
        "    "
    };
    let insertion = format!(
        "{comma}\n{indent}{property}\n{}",
        " ".repeat(indent.len().saturating_sub(2))
    );
    format!(
        "{}{}{}",
        &text[..object_end],
        insertion,
        &text[object_end..]
    )
}

fn find_object_property(
    text: &str,
    key: &str,
    object_start_hint: usize,
    object_end_hint: usize,
) -> Option<(usize, usize, std::ops::Range<usize>)> {
    // 返回值同时给出 value 的对象边界和整条属性范围；删除时需要整条属性，
    // 插入子项时需要对象内部边界。
    let object_start = if text.as_bytes().get(object_start_hint) == Some(&b'{') {
        object_start_hint
    } else {
        text[object_start_hint..object_end_hint].find('{')? + object_start_hint
    };
    let object_end = find_matching_brace(text, object_start)?.min(object_end_hint);
    let mut cursor = object_start + 1;
    while cursor < object_end {
        cursor = skip_ws_comments(text, cursor, object_end);
        if cursor >= object_end || text.as_bytes().get(cursor) == Some(&b'}') {
            break;
        }
        if text.as_bytes().get(cursor) == Some(&b',') {
            cursor += 1;
            continue;
        }
        if text.as_bytes().get(cursor) != Some(&b'"') {
            cursor += 1;
            continue;
        }
        let key_start = cursor;
        let key_end = parse_string_end(text, key_start)?;
        let parsed_key = &text[key_start + 1..key_end - 1];
        cursor = skip_ws_comments(text, key_end, object_end);
        if text.as_bytes().get(cursor) != Some(&b':') {
            continue;
        }
        cursor = skip_ws_comments(text, cursor + 1, object_end);
        let value_start = cursor;
        let value_end = parse_value_end(text, value_start, object_end)?;
        if parsed_key == key {
            if text.as_bytes().get(value_start) == Some(&b'{') {
                let object_value_end = find_matching_brace(text, value_start)?;
                return Some((value_start, object_value_end, key_start..value_end));
            }
            return Some((value_start, value_end, key_start..value_end));
        }
        cursor = value_end;
    }
    None
}

fn skip_ws_comments(text: &str, mut idx: usize, end: usize) -> usize {
    let bytes = text.as_bytes();
    while idx < end {
        if bytes[idx].is_ascii_whitespace() {
            idx += 1;
        } else if bytes.get(idx..idx + 2) == Some(b"//") {
            idx = text[idx..]
                .find('\n')
                .map(|rel| idx + rel + 1)
                .unwrap_or(end);
        } else if bytes.get(idx..idx + 2) == Some(b"/*") {
            idx = text[idx + 2..]
                .find("*/")
                .map(|rel| idx + rel + 4)
                .unwrap_or(end);
        } else {
            break;
        }
    }
    idx
}

fn parse_string_end(text: &str, start: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut idx = start + 1;
    let mut escaped = false;
    while idx < bytes.len() {
        let b = bytes[idx];
        if escaped {
            escaped = false;
        } else if b == b'\\' {
            escaped = true;
        } else if b == b'"' {
            return Some(idx + 1);
        }
        idx += 1;
    }
    None
}

fn parse_value_end(text: &str, start: usize, end: usize) -> Option<usize> {
    match text.as_bytes().get(start).copied()? {
        b'"' => parse_string_end(text, start),
        b'{' | b'[' => find_matching_container(text, start),
        _ => {
            let mut idx = start;
            while idx < end && !matches!(text.as_bytes()[idx], b',' | b'}' | b']' | b'\n') {
                idx += 1;
            }
            Some(idx)
        }
    }
}

fn find_matching_container(text: &str, start: usize) -> Option<usize> {
    let open = *text.as_bytes().get(start)?;
    let close = if open == b'{' { b'}' } else { b']' };
    let mut idx = start;
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    let bytes = text.as_bytes();
    while idx < bytes.len() {
        let b = bytes[idx];
        if in_string {
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == b'"' {
                in_string = false;
            }
        } else if b == b'"' {
            in_string = true;
        } else if b == open {
            depth += 1;
        } else if b == close {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                return Some(idx + 1);
            }
        }
        idx += 1;
    }
    None
}

fn find_matching_brace(text: &str, start: usize) -> Option<usize> {
    find_matching_container(text, start).map(|idx| idx - 1)
}

fn remove_range_with_comma(text: &str, range: std::ops::Range<usize>) -> String {
    // 删除属性时要同时吃掉前/后的逗号，否则 JSONC 会留下 dangling comma
    // 或空行碎片。
    let bytes = text.as_bytes();
    let mut start = range.start;
    let mut end = range.end;
    let mut probe = end;
    while probe < bytes.len() && bytes[probe].is_ascii_whitespace() && bytes[probe] != b'\n' {
        probe += 1;
    }
    if bytes.get(probe) == Some(&b',') {
        end = probe + 1;
    } else {
        probe = start;
        while probe > 0 && bytes[probe - 1].is_ascii_whitespace() && bytes[probe - 1] != b'\n' {
            probe -= 1;
        }
        if probe > 0 && bytes[probe - 1] == b',' {
            start = probe - 1;
        }
    }
    let line_start = text[..start].rfind('\n').map(|idx| idx + 1).unwrap_or(0);
    let line_end = text[end..]
        .find('\n')
        .map(|idx| end + idx + 1)
        .unwrap_or(end);
    format!("{}{}", &text[..line_start], &text[line_end..])
}

fn strip_jsonc_comments(text: &str) -> String {
    let mut out = String::new();
    let bytes = text.as_bytes();
    let mut idx = 0;
    let mut in_string = false;
    let mut escaped = false;
    while idx < bytes.len() {
        let b = bytes[idx];
        if in_string {
            out.push(b as char);
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == b'"' {
                in_string = false;
            }
            idx += 1;
        } else if b == b'"' {
            in_string = true;
            out.push('"');
            idx += 1;
        } else if bytes.get(idx..idx + 2) == Some(b"//") {
            idx = text[idx..]
                .find('\n')
                .map(|rel| idx + rel)
                .unwrap_or(bytes.len());
        } else if bytes.get(idx..idx + 2) == Some(b"/*") {
            idx = text[idx + 2..]
                .find("*/")
                .map(|rel| idx + rel + 4)
                .unwrap_or(bytes.len());
        } else {
            out.push(b as char);
            idx += 1;
        }
    }
    out
}

fn cleanup_legacy_windows_state() -> Vec<FileWrite> {
    // 全局安装/卸载都会顺手清理 legacy Windows 目录中的 RustCodeGraph 状态；
    // 只报告实际移除的文件，避免 CLI 输出充满无关 not-found。
    let Some(dir) = legacy_windows_config_dir() else {
        return Vec::new();
    };
    if !dir.exists() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for name in ["opencode.jsonc", "opencode.json"] {
        let res = remove_mcp_entry_at(dir.join(name));
        if res.action == WriteAction::Removed {
            out.push(res);
        }
    }
    let agents = dir.join("AGENTS.md");
    let res = remove_rustcodegraph_instructions(agents);
    if res.action == WriteAction::Removed {
        out.push(res);
    }
    out
}

fn remove_instructions_entry(loc: Location) -> FileWrite {
    remove_rustcodegraph_instructions(instructions_path(loc))
}
