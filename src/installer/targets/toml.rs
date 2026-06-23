//! Narrow TOML helpers for `[mcp_servers.rustcodegraph]`.
//!
//! 这些函数不是通用 TOML 解析器，只识别一个普通 table 的边界。
//! 这样可以不新增依赖，并保留用户文件中的注释、顺序和不相关表。

use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TomlValue {
    String(String),
    Strings(Vec<String>),
}

pub fn serialize_toml_table_body(values: &BTreeMap<String, TomlValue>) -> String {
    values
        .iter()
        .map(|(key, value)| match value {
            TomlValue::String(value) => format!("{key} = {}", quote_string(value)),
            TomlValue::Strings(values) => {
                let parts = values
                    .iter()
                    .map(|value| quote_string(value))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{key} = [{parts}]")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn quote_string(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

pub fn build_toml_table(header: &str, values: BTreeMap<String, TomlValue>) -> String {
    format!("[{header}]\n{}", serialize_toml_table_body(&values))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpsertTomlAction {
    Inserted,
    Replaced,
    Unchanged,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpsertTomlResult {
    pub content: String,
    pub action: UpsertTomlAction,
}

pub fn upsert_toml_table(file_content: &str, header: &str, block: &str) -> UpsertTomlResult {
    let header_line = format!("[{header}]");
    let Some(header_idx) = find_header_index(file_content, &header_line) else {
        let trimmed = file_content.trim_end();
        let sep = if trimmed.is_empty() { "" } else { "\n\n" };
        return UpsertTomlResult {
            content: format!("{trimmed}{sep}{block}\n"),
            action: UpsertTomlAction::Inserted,
        };
    };

    let block_end = find_next_table_header(file_content, header_idx + header_line.len());
    let existing_block = file_content[header_idx..block_end].trim_end_matches('\n');
    if existing_block == block {
        return UpsertTomlResult {
            content: file_content.to_owned(),
            action: UpsertTomlAction::Unchanged,
        };
    }

    let before = file_content[..header_idx].trim_end_matches('\n');
    let after = file_content[block_end..].trim_start_matches('\n');
    let sep_before = if before.is_empty() { "" } else { "\n\n" };
    let sep_after = if after.is_empty() { "\n" } else { "\n\n" };
    UpsertTomlResult {
        content: format!("{before}{sep_before}{block}{sep_after}{after}"),
        action: UpsertTomlAction::Replaced,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoveTomlAction {
    Removed,
    NotFound,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoveTomlResult {
    pub content: String,
    pub action: RemoveTomlAction,
}

pub fn remove_toml_table(file_content: &str, header: &str) -> RemoveTomlResult {
    let header_line = format!("[{header}]");
    let Some(header_idx) = find_header_index(file_content, &header_line) else {
        return RemoveTomlResult {
            content: file_content.to_owned(),
            action: RemoveTomlAction::NotFound,
        };
    };
    let block_end = find_next_table_header(file_content, header_idx + header_line.len());
    let before = file_content[..header_idx].trim_end_matches('\n');
    let after = file_content[block_end..].trim_start_matches('\n');
    let sep = if before.is_empty() || after.is_empty() {
        ""
    } else {
        "\n\n"
    };
    RemoveTomlResult {
        content: format!("{before}{sep}{after}"),
        action: RemoveTomlAction::Removed,
    }
}

fn find_header_index(content: &str, header_line: &str) -> Option<usize> {
    if content.starts_with(header_line) {
        return Some(0);
    }
    content.find(&format!("\n{header_line}")).map(|idx| idx + 1)
}

fn find_next_table_header(content: &str, from: usize) -> usize {
    // `[[array_of_tables]]` 不是 sibling `[table]`，不能把它当作本表结束后的
    // 可替换目标；遇到双中括号继续向后找普通 table。
    let mut cursor = from;
    while cursor < content.len() {
        let Some(rel) = content[cursor..].find("\n[") else {
            return content.len();
        };
        let idx = cursor + rel;
        if content.as_bytes().get(idx + 2) == Some(&b'[') {
            cursor = idx + 2;
            continue;
        }
        return idx + 1;
    }
    content.len()
}
