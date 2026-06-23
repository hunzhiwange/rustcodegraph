//! facade 节点构建和声明头解析。
//!
//! 本模块统一生成节点 id、基础字段和 qualified_name，确保原生抽取补洞与纯 fallback 抽取的节点形态一致。

use super::*;

pub(super) fn facade_file_node(
    file_path: &str,
    source: &str,
    language: Language,
    indexed_at: TimestampMs,
) -> Node {
    let line_count = source.lines().count().max(1) as u64;
    Node {
        id: format!("file:{file_path}"),
        kind: NodeKind::File,
        name: file_path.rsplit('/').next().unwrap_or(file_path).to_owned(),
        qualified_name: file_path.to_owned(),
        file_path: file_path.to_owned(),
        language,
        start_line: 1,
        end_line: line_count,
        start_column: 0,
        end_column: 0,
        docstring: None,
        signature: None,
        visibility: None,
        is_exported: Some(false),
        is_async: Some(false),
        is_static: Some(false),
        is_abstract: Some(false),
        decorators: None,
        type_parameters: None,
        return_type: None,
        updated_at: indexed_at,
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn facade_node(
    file_path: &str,
    language: Language,
    name: &str,
    kind: NodeKind,
    line: &str,
    line_number: u64,
    signature: Option<String>,
    visibility: Option<Visibility>,
    is_static: bool,
    indexed_at: TimestampMs,
) -> Node {
    Node {
        id: format!(
            "{}:{}:{}:{}",
            file_path,
            line_number,
            kind_key(kind),
            sanitize_id_part(name)
        ),
        kind,
        name: name.to_owned(),
        qualified_name: format!("{file_path}::{name}"),
        file_path: file_path.to_owned(),
        language,
        start_line: line_number,
        end_line: line_number,
        start_column: 0,
        end_column: line.len() as u64,
        docstring: None,
        signature,
        visibility,
        is_exported: Some(line.trim_start().starts_with("export ")),
        is_async: Some(line.contains("async ")),
        is_static: Some(is_static),
        is_abstract: Some(line.contains("abstract ")),
        decorators: None,
        type_parameters: None,
        return_type: None,
        updated_at: indexed_at,
    }
}

pub(super) fn apply_facade_qualified_name(
    node: &mut Node,
    package_prefix: Option<&str>,
    class_name: Option<&str>,
) {
    // package/namespace 前缀优先于文件路径；类成员再追加当前类名，便于跨文件名称解析。
    let prefix = package_prefix
        .map(str::trim)
        .filter(|prefix| !prefix.is_empty());
    node.qualified_name = match (prefix, class_name) {
        (Some(prefix), Some(class_name)) => format!("{prefix}::{class_name}::{}", node.name),
        (Some(prefix), None) => format!("{prefix}::{}", node.name),
        (None, Some(class_name)) => format!("{}::{class_name}::{}", node.file_path, node.name),
        (None, None) => node.qualified_name.clone(),
    };
}

pub(super) fn container_from_line(line: &str) -> Option<(&str, FacadeContainerKind)> {
    // 只识别声明头，不消费 body；调用方负责根据 brace 深度维护类/对象上下文。
    let mut rest = line.trim_start();
    if let Some(after_objc) = rest.strip_prefix("@implementation ") {
        return first_identifier(after_objc)
            .map(|name| (trim_identifier(name), FacadeContainerKind::Class));
    }
    if let Some(after_namespace) = rest.strip_prefix("namespace ")
        && let Some((_, after_brace)) = after_namespace.split_once('{')
    {
        rest = after_brace.trim_start();
    }

    loop {
        let mut changed = false;
        for prefix in [
            "export ",
            "default ",
            "abstract ",
            "public ",
            "private ",
            "protected ",
            "pub ",
            "open ",
            "data ",
            "sealed ",
        ] {
            if let Some(after) = rest.strip_prefix(prefix) {
                rest = after.trim_start();
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    if let Some(after_type) = rest.strip_prefix("type ") {
        let name = first_identifier(after_type).map(trim_identifier)?;
        let tail = after_type[name.len()..].trim_start();
        if tail.starts_with("struct") {
            return Some((name, FacadeContainerKind::Struct));
        }
        if tail.starts_with("interface") {
            return Some((name, FacadeContainerKind::Class));
        }
    }

    if let Some(after_class) = rest.strip_prefix("class ") {
        return first_identifier(after_class)
            .map(|name| (trim_identifier(name), FacadeContainerKind::Class));
    }
    if let Some(after_trait) = rest.strip_prefix("trait ") {
        return first_identifier(after_trait)
            .map(|name| (trim_identifier(name), FacadeContainerKind::Trait));
    }
    if let Some(after_object) = rest.strip_prefix("object ") {
        return first_identifier(after_object)
            .map(|name| (trim_identifier(name), FacadeContainerKind::Object));
    }
    if let Some(after_struct) = rest.strip_prefix("struct ") {
        return first_identifier(after_struct)
            .map(|name| (trim_identifier(name), FacadeContainerKind::Struct));
    }
    if let Some(after_enum) = rest.strip_prefix("enum ") {
        return first_identifier(after_enum)
            .map(|name| (trim_identifier(name), FacadeContainerKind::Enum));
    }
    None
}

pub(super) fn interface_name_from_line(line: &str) -> Option<&str> {
    let mut parts = line.split_whitespace().peekable();
    while matches!(parts.peek(), Some(&"export" | &"default" | &"declare")) {
        parts.next();
    }
    if parts.next()? != "interface" {
        return None;
    }
    parts.next().map(trim_identifier)
}

pub(super) fn class_relation_names_from_line(line: &str, keyword: &str) -> Vec<String> {
    // 继承/实现列表可能后面接另一个关系子句，截断到下一个关键字避免把后续 token 当成类型名。
    let Some(keyword_start) = find_keyword(line, keyword) else {
        return Vec::new();
    };
    let after_keyword = line[keyword_start + keyword.len()..].trim_start();
    let relation_end = after_keyword
        .find('{')
        .or_else(|| after_keyword.find(" implements "))
        .or_else(|| after_keyword.find(" extends "))
        .or_else(|| after_keyword.find(" with "))
        .unwrap_or(after_keyword.len());
    after_keyword[..relation_end]
        .split(',')
        .filter_map(|raw| first_identifier(raw).map(trim_identifier))
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .collect()
}

pub(super) fn find_keyword(line: &str, keyword: &str) -> Option<usize> {
    let pattern = format!(" {keyword} ");
    line.find(&pattern).map(|idx| idx + 1)
}
