//! 轻量 CLI 索引器的正则符号抽取。
//!
//! 这里故意只识别高置信度声明形态；宁可漏掉复杂语法，也不制造会污染 callers/
//! affected 结果的大量误报。完整语言覆盖由库里的 tree-sitter extractor 负责。

use std::sync::LazyLock;

use regex::Regex;
use rustcodegraph::types::{Language, LineNumber, Node, NodeKind, TimestampMs};

pub(super) fn extract_lightweight_symbols(
    file_path: &str,
    source: &str,
    language: Language,
    indexed_at: TimestampMs,
) -> Vec<Node> {
    let mut nodes = Vec::new();
    for (idx, line) in source.lines().enumerate() {
        let line_number = (idx + 1) as LineNumber;
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with('#') {
            continue;
        }
        // 每个语言分支只扫描单行声明；多行签名和嵌套类型会留给完整索引器。
        match language {
            Language::Rust => {
                push_regex_node(
                    &mut nodes,
                    &RUST_FN_RE,
                    1,
                    NodeKind::Function,
                    file_path,
                    language,
                    line,
                    line_number,
                    indexed_at,
                );
                push_type_regex_node(
                    &mut nodes,
                    &RUST_TYPE_RE,
                    1,
                    2,
                    file_path,
                    language,
                    line,
                    line_number,
                    indexed_at,
                );
            }
            Language::TypeScript | Language::JavaScript | Language::Tsx | Language::Jsx => {
                push_regex_node(
                    &mut nodes,
                    &TS_FN_RE,
                    1,
                    NodeKind::Function,
                    file_path,
                    language,
                    line,
                    line_number,
                    indexed_at,
                );
                push_type_regex_node(
                    &mut nodes,
                    &TS_TYPE_RE,
                    1,
                    2,
                    file_path,
                    language,
                    line,
                    line_number,
                    indexed_at,
                );
                push_regex_node(
                    &mut nodes,
                    &TS_ARROW_RE,
                    1,
                    NodeKind::Function,
                    file_path,
                    language,
                    line,
                    line_number,
                    indexed_at,
                );
            }
            Language::Python => {
                push_regex_node(
                    &mut nodes,
                    &PY_CLASS_RE,
                    1,
                    NodeKind::Class,
                    file_path,
                    language,
                    line,
                    line_number,
                    indexed_at,
                );
                push_regex_node(
                    &mut nodes,
                    &PY_FN_RE,
                    1,
                    NodeKind::Function,
                    file_path,
                    language,
                    line,
                    line_number,
                    indexed_at,
                );
            }
            Language::Go => {
                push_regex_node(
                    &mut nodes,
                    &GO_FN_RE,
                    1,
                    NodeKind::Function,
                    file_path,
                    language,
                    line,
                    line_number,
                    indexed_at,
                );
                push_type_regex_node(
                    &mut nodes,
                    &GO_TYPE_RE,
                    2,
                    1,
                    file_path,
                    language,
                    line,
                    line_number,
                    indexed_at,
                );
            }
            Language::Ruby => {
                push_regex_node(
                    &mut nodes,
                    &RUBY_TYPE_RE,
                    2,
                    NodeKind::Class,
                    file_path,
                    language,
                    line,
                    line_number,
                    indexed_at,
                );
                push_regex_node(
                    &mut nodes,
                    &RUBY_FN_RE,
                    1,
                    NodeKind::Function,
                    file_path,
                    language,
                    line,
                    line_number,
                    indexed_at,
                );
            }
            _ => {
                push_type_regex_node(
                    &mut nodes,
                    &CLASS_LIKE_RE,
                    1,
                    2,
                    file_path,
                    language,
                    line,
                    line_number,
                    indexed_at,
                );
                push_regex_node(
                    &mut nodes,
                    &GENERIC_FN_RE,
                    1,
                    NodeKind::Function,
                    file_path,
                    language,
                    line,
                    line_number,
                    indexed_at,
                );
            }
        }
    }
    dedupe_nodes(nodes)
}

#[allow(clippy::too_many_arguments)]
fn push_regex_node(
    nodes: &mut Vec<Node>,
    regex: &Regex,
    name_group: usize,
    kind: NodeKind,
    file_path: &str,
    language: Language,
    line: &str,
    line_number: LineNumber,
    indexed_at: TimestampMs,
) {
    let Some(captures) = regex.captures(line) else {
        return;
    };
    let Some(name) = captures.get(name_group).map(|m| m.as_str()) else {
        return;
    };
    nodes.push(symbol_node(
        file_path,
        name,
        kind,
        language,
        line,
        line_number,
        indexed_at,
    ));
}

#[allow(clippy::too_many_arguments)]
fn push_type_regex_node(
    nodes: &mut Vec<Node>,
    regex: &Regex,
    kind_group: usize,
    name_group: usize,
    file_path: &str,
    language: Language,
    line: &str,
    line_number: LineNumber,
    indexed_at: TimestampMs,
) {
    let Some(captures) = regex.captures(line) else {
        return;
    };
    let Some(raw_kind) = captures.get(kind_group).map(|m| m.as_str()) else {
        return;
    };
    let Some(name) = captures.get(name_group).map(|m| m.as_str()) else {
        return;
    };
    nodes.push(symbol_node(
        file_path,
        name,
        node_kind_from_decl(raw_kind),
        language,
        line,
        line_number,
        indexed_at,
    ));
}

pub(super) fn file_node(
    file_path: &str,
    source: &str,
    language: Language,
    indexed_at: TimestampMs,
) -> Node {
    // file 节点让无函数声明的文件也能参与 affected/test 查询和 MCP file-mode 输出。
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

fn symbol_node(
    file_path: &str,
    name: &str,
    kind: NodeKind,
    language: Language,
    line: &str,
    line_number: LineNumber,
    indexed_at: TimestampMs,
) -> Node {
    // ID 包含文件、行号、kind 和清洗后的名字；轻量索引没有 AST byte range，
    // 这个组合在重建索引时稳定，也足够区分同文件常见重载/同名声明。
    let qualified_name = format!("{}::{}", file_path.trim_end_matches('/'), name);
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
        qualified_name,
        file_path: file_path.to_owned(),
        language,
        start_line: line_number,
        end_line: line_number,
        start_column: 0,
        end_column: line.len() as u64,
        docstring: None,
        signature: Some(line.trim().to_owned()),
        visibility: None,
        is_exported: Some(line.contains("pub ") || line.trim_start().starts_with("export ")),
        is_async: Some(line.contains("async ")),
        is_static: Some(line.contains("static ")),
        is_abstract: Some(line.contains("abstract ")),
        decorators: None,
        type_parameters: None,
        return_type: None,
        updated_at: indexed_at,
    }
}

fn dedupe_nodes(nodes: Vec<Node>) -> Vec<Node> {
    // 同一行可能同时匹配语言专用和通用正则，按 ID 去重保持插入事务幂等。
    let mut seen = std::collections::HashSet::new();
    nodes
        .into_iter()
        .filter(|node| seen.insert(node.id.clone()))
        .collect()
}

fn node_kind_from_decl(kind: &str) -> NodeKind {
    match kind {
        "class" => NodeKind::Class,
        "interface" => NodeKind::Interface,
        "enum" => NodeKind::Enum,
        "struct" => NodeKind::Struct,
        "trait" => NodeKind::Trait,
        "protocol" => NodeKind::Protocol,
        "type" => NodeKind::TypeAlias,
        "module" => NodeKind::Module,
        _ => NodeKind::Class,
    }
}

fn kind_key(kind: NodeKind) -> &'static str {
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

fn sanitize_id_part(value: &str) -> String {
    // SQLite 主键可以存任意字符串，但下游调试和 MCP 输出更适合可读的 ASCII-ish ID。
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

// 正则放在 LazyLock 中，普通 CLI 启动不会为索引专用规则付编译成本。
static RUST_FN_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?(?:unsafe\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)",
    )
    .unwrap()
});
static RUST_TYPE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*(?:pub(?:\([^)]*\))?\s+)?(struct|enum|trait)\s+([A-Za-z_][A-Za-z0-9_]*)")
        .unwrap()
});
static TS_FN_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^\s*(?:export\s+)?(?:default\s+)?(?:async\s+)?function\s+([A-Za-z_$][A-Za-z0-9_$]*)",
    )
    .unwrap()
});
static TS_TYPE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*(?:export\s+)?(?:default\s+)?(class|interface|enum|type)\s+([A-Za-z_$][A-Za-z0-9_$]*)").unwrap()
});
static TS_ARROW_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*(?:export\s+)?(?:const|let|var)\s+([A-Za-z_$][A-Za-z0-9_$]*)\s*=\s*(?:async\s*)?(?:\([^)]*\)|[A-Za-z_$][A-Za-z0-9_$]*)\s*=>").unwrap()
});
static PY_CLASS_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*class\s+([A-Za-z_][A-Za-z0-9_]*)").unwrap());
static PY_FN_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*(?:async\s+)?def\s+([A-Za-z_][A-Za-z0-9_]*)").unwrap());
static GO_FN_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*func\s+(?:\([^)]*\)\s*)?([A-Za-z_][A-Za-z0-9_]*)\s*\(").unwrap()
});
static GO_TYPE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*type\s+([A-Za-z_][A-Za-z0-9_]*)\s+(struct|interface)").unwrap()
});
static RUBY_TYPE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*(class|module)\s+([A-Za-z_][A-Za-z0-9_:]*)").unwrap());
static RUBY_FN_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*def\s+([A-Za-z_][A-Za-z0-9_!?=]*)").unwrap());
static CLASS_LIKE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*(?:public|private|protected|internal|open|final|abstract|sealed|export|static|\s)*\s*(class|interface|enum|struct|trait|protocol)\s+([A-Za-z_][A-Za-z0-9_]*)").unwrap()
});
static GENERIC_FN_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*(?:public|private|protected|internal|static|final|open|async|export|\s)*\s*(?:function\s+)?([A-Za-z_][A-Za-z0-9_]*)\s*\([^;{}]*\)\s*(?:\{|$)").unwrap()
});
