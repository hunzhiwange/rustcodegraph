//! Shared helpers for MCP tool implementations.
//!
//! 这些函数保持工具输出的一致排序、路径匹配和标签映射。它们不访问 MCP transport，
//! 只处理 CodeGraph 查询结果到面向 agent 的文本之间的细节。

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use regex::Regex;

use crate::types::{EdgeKind, Language, Node, NodeKind};

pub(super) fn find_symbol_matches(cg: &mut crate::CodeGraph, symbol: &str) -> Vec<Node> {
    let symbol = symbol.trim();
    if symbol.is_empty() {
        return Vec::new();
    }
    let is_qualified = symbol.contains('.') || symbol.contains('/') || symbol.contains("::");
    if !is_qualified {
        // 普通短名先拿所有精确定义，让 node/graph_query 能一次展示 overload；
        // 只有没有精确命中时才退到 FTS 的首个候选。
        let mut exact = cg.get_nodes_by_name(symbol);
        if !exact.is_empty() {
            sort_nodes_for_output(&mut exact);
            return exact;
        }
        return cg
            .search_nodes(symbol, None)
            .into_iter()
            .next()
            .map(|result| vec![result.node])
            .unwrap_or_default();
    }

    let mut results = cg.search_nodes(symbol, None);
    if results.is_empty()
        && let Some(tail) = last_qualifier_part(symbol).filter(|tail| *tail != symbol)
    {
        // 用户常传 `Class.method`、`pkg/path.Symbol` 这类限定名；索引里可能只存
        // 尾段 name，所以先宽搜 tail，再用 qualified_name 做保守过滤。
        results = cg.search_nodes(tail, None);
    }
    let mut matches = results
        .into_iter()
        .map(|result| result.node)
        .filter(|node| node_matches_symbol(node, symbol))
        .collect::<Vec<_>>();
    sort_nodes_for_output(&mut matches);
    matches
}

pub(super) fn edge_kind_label(kind: EdgeKind) -> &'static str {
    match kind {
        EdgeKind::Contains => "contains",
        EdgeKind::Calls => "calls",
        EdgeKind::Imports => "imports",
        EdgeKind::Exports => "exports",
        EdgeKind::Extends => "extends",
        EdgeKind::Implements => "implements",
        EdgeKind::References => "references",
        EdgeKind::TypeOf => "type_of",
        EdgeKind::Returns => "returns",
        EdgeKind::Instantiates => "instantiates",
        EdgeKind::Overrides => "overrides",
        EdgeKind::Decorates => "decorates",
    }
}

pub(super) fn file_hint_matches(file_path: &str, hint: &str) -> bool {
    // 兼容绝对/相对、Windows 分隔符和 basename hint；多定义符号需要靠这个缩小
    // 到用户正在看的文件。
    let path = normalize_slashes(file_path);
    let hint = normalize_slashes(hint);
    path == hint || path.ends_with(&format!("/{hint}")) || path.ends_with(&hint)
}

pub(super) fn same_monorepo_scope(a: &str, b: &str) -> bool {
    // apps/* monorepo 中同名服务很多；跨 app 合并 callers/callees 会制造假流。
    match (apps_scope(a), apps_scope(b)) {
        (Some(left), Some(right)) => left == right,
        (Some(_), None) | (None, Some(_)) => false,
        (None, None) => true,
    }
}

pub(super) fn apps_scope(path: &str) -> Option<String> {
    let mut parts = path.split('/');
    if parts.next()? != "apps" {
        return None;
    }
    Some(format!("apps/{}", parts.next()?))
}

pub(super) fn sort_node_edge_rows(rows: &mut [(Node, crate::types::Edge)]) {
    rows.sort_by(|(a, _), (b, _)| {
        a.file_path
            .cmp(&b.file_path)
            .then_with(|| a.start_line.cmp(&b.start_line))
            .then_with(|| a.name.cmp(&b.name))
    });
}

pub(super) fn dedupe_node_edge_rows(rows: &mut Vec<(Node, crate::types::Edge)>) {
    let mut seen = HashSet::new();
    rows.retain(|(node, _)| seen.insert(node.id.clone()));
}

pub(super) fn sort_nodes_for_output(nodes: &mut [Node]) {
    nodes.sort_by(|a, b| {
        a.file_path
            .cmp(&b.file_path)
            .then_with(|| a.start_line.cmp(&b.start_line))
            .then_with(|| a.name.cmp(&b.name))
    });
}

pub(super) fn dedupe_nodes(nodes: &mut Vec<Node>) {
    let mut seen = HashSet::new();
    nodes.retain(|node| seen.insert(node.id.clone()));
}

pub(super) fn last_qualifier_part(symbol: &str) -> Option<&str> {
    symbol.rsplit(['.', '/', ':']).find(|part| !part.is_empty())
}

pub(super) fn node_matches_symbol(node: &Node, symbol: &str) -> bool {
    if node.name == symbol || node.qualified_name == symbol {
        return true;
    }
    let normalized_symbol = symbol.replace("::", ".").replace('/', ".");
    let normalized_qualified = node.qualified_name.replace("::", ".").replace('/', ".");
    normalized_qualified.ends_with(&normalized_symbol)
        || normalized_qualified
            .to_ascii_lowercase()
            .ends_with(&normalized_symbol.to_ascii_lowercase())
}

pub fn glob_to_regex(pattern: &str) -> Result<Regex, regex::Error> {
    // 这是 files 工具的轻量 glob：`**` 跨目录，`*` 和 `?` 不跨 `/`。
    let mut escaped = String::new();
    let chars: Vec<char> = pattern.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '*' if chars.get(i + 1) == Some(&'*') => {
                escaped.push_str(".*");
                i += 2;
            }
            '*' => {
                escaped.push_str("[^/]*");
                i += 1;
            }
            '?' => {
                escaped.push_str("[^/]");
                i += 1;
            }
            c => {
                escaped.push_str(&regex::escape(&c.to_string()));
                i += 1;
            }
        }
    }
    Regex::new(&escaped)
}

pub(super) fn language_label(language: Language) -> String {
    serde_json::to_value(language)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_else(|| "unknown".to_string())
}

pub(super) fn project_file_path(project_root: &Path, file_path: &str) -> PathBuf {
    // DB 中通常是项目相对路径；少数调用者会传绝对路径，避免重复 join。
    let path = Path::new(file_path);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        project_root.join(path)
    }
}

pub(super) fn config_keys(source: &str) -> Vec<String> {
    // MCP 输出配置文件时只暴露 key，不暴露 value，避免把密钥/连接串带进上下文。
    let mut keys = Vec::new();
    let mut seen = HashSet::new();
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('!') {
            continue;
        }
        let key = trimmed
            .split_once('=')
            .map(|(key, _)| key)
            .or_else(|| trimmed.split_once(':').map(|(key, _)| key))
            .unwrap_or(trimmed)
            .trim();
        if key.is_empty() || key.starts_with('-') {
            continue;
        }
        if seen.insert(key.to_string()) {
            keys.push(key.to_string());
        }
    }
    keys
}

pub(super) fn normalize_slashes(path: &str) -> String {
    path.replace('\\', "/")
}

pub(super) fn node_matches_query(node: &Node, needle: &str) -> bool {
    node.name.to_ascii_lowercase().contains(needle)
        || node.qualified_name.to_ascii_lowercase().contains(needle)
        || node.file_path.to_ascii_lowercase().contains(needle)
}

pub(super) fn node_kind_filter(raw: &str) -> Option<NodeKind> {
    // Tool schema 暴露的是面向用户的短 kind；这里映射到内部枚举。
    match raw {
        "function" => Some(NodeKind::Function),
        "method" => Some(NodeKind::Method),
        "class" => Some(NodeKind::Class),
        "interface" => Some(NodeKind::Interface),
        "type" | "type_alias" => Some(NodeKind::TypeAlias),
        "variable" => Some(NodeKind::Variable),
        "route" => Some(NodeKind::Route),
        "component" => Some(NodeKind::Component),
        _ => None,
    }
}

pub(super) fn node_kind_label(kind: NodeKind) -> &'static str {
    match kind {
        NodeKind::File => "file",
        NodeKind::Module => "module",
        NodeKind::Class => "class",
        NodeKind::Struct => "struct",
        NodeKind::Interface => "interface",
        NodeKind::Trait => "trait",
        NodeKind::Protocol => "protocol",
        NodeKind::Function => "function",
        NodeKind::Method => "method",
        NodeKind::Property => "property",
        NodeKind::Field => "field",
        NodeKind::Variable => "variable",
        NodeKind::Constant => "constant",
        NodeKind::Enum => "enum",
        NodeKind::EnumMember => "enum_member",
        NodeKind::TypeAlias => "type_alias",
        NodeKind::Namespace => "namespace",
        NodeKind::Parameter => "parameter",
        NodeKind::Import => "import",
        NodeKind::Export => "export",
        NodeKind::Route => "route",
        NodeKind::Component => "component",
    }
}
