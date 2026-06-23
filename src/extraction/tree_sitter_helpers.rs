//! Shared tree-sitter helpers.
//!
//! 这些 helper 位于语言适配器和核心抽取器之间，负责统一节点 ID、源码切片和
//! docstring 归一化。它们刻意保持无状态，方便多语言抽取器复用同一套边界行为。

use crate::types::NodeKind;
use crate::web_tree_sitter::SyntaxNode;
use sha2::{Digest, Sha256};

/// Generate the same 32-character SHA-256 based node id shape as the TS core:
/// `<kind>:<128-bit hex prefix>`.
/// 节点 ID 必须跨运行稳定；索引更新、引用解析和 SQLite 去重都依赖同一输入
/// 生成同一 ID，因此这里不要混入时间戳或 AST 中不稳定的字段。
pub fn generate_node_id(file_path: &str, kind: NodeKind, name: &str, line: usize) -> String {
    let mut hasher = Sha256::new();
    let kind_key = node_kind_key(kind);
    hasher.update(format!("{file_path}:{kind_key}:{name}:{line}"));
    let digest = hasher.finalize();
    let hex = format!("{digest:x}");
    format!("{kind_key}:{}", &hex[..32])
}

fn node_kind_key(kind: NodeKind) -> &'static str {
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

pub fn get_node_text<N>(node: N, source: &str) -> String
where
    N: AsRef<SyntaxNode>,
{
    let node = node.as_ref();
    source
        .get(node.start_index..node.end_index)
        .unwrap_or("")
        .to_owned()
}

pub fn get_child_by_field<'a>(node: &'a SyntaxNode, field_name: &str) -> Option<&'a SyntaxNode> {
    node.child_for_field_name(field_name)
}

const DOCSTRING_WRAPPER_TYPES: &[&str] = &[
    "export_statement",
    "decorated_definition",
    "lexical_declaration",
    "variable_declaration",
    "variable_declarator",
    "ambient_declaration",
];

fn clean_comment_markers(comment: &str) -> String {
    // 多语言注释标记在这里被折叠成纯文本。保留换行可以让 context 输出中
    // 的文档注释仍然可读，但去掉前缀噪音，避免污染搜索和 MCP 展示。
    let mut c = comment.trim().to_owned();
    if c.starts_with("/*") {
        c = c
            .trim_start_matches('/')
            .trim_start_matches('*')
            .trim_start_matches('!')
            .trim_end_matches('/')
            .trim_end_matches('*')
            .to_owned();
    } else if c.starts_with("--[") {
        c = c
            .trim_start_matches('-')
            .trim_start_matches('[')
            .trim_end_matches(']')
            .to_owned();
    } else if c.starts_with("(*") {
        c = c
            .trim_start_matches('(')
            .trim_start_matches('*')
            .trim_end_matches(')')
            .trim_end_matches('*')
            .to_owned();
    } else if c.starts_with('{') {
        c = c.trim_start_matches('{').trim_end_matches('}').to_owned();
    }

    c.lines()
        .map(|line| {
            line.trim_start()
                .trim_start_matches("///")
                .trim_start_matches("//!")
                .trim_start_matches("//")
                .trim_start_matches("--")
                .trim_start_matches('#')
                .trim_start_matches('*')
                .trim_start()
        })
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_owned()
}

pub fn get_preceding_docstring(node: &SyntaxNode, source: &str) -> Option<String> {
    // 一些 grammar 会把导出、装饰器或变量声明包在外层节点里；先上移到稳定
    // anchor，再找紧邻注释，才能把 docstring 挂到真正声明上。
    let mut anchor = node;
    while let Some(parent) = anchor.parent.as_deref() {
        if DOCSTRING_WRAPPER_TYPES.contains(&parent.node_type()) {
            anchor = parent;
        } else {
            break;
        }
    }

    let mut comments = Vec::new();
    let mut sibling = anchor.previous_named_sibling.as_deref();
    while let Some(current) = sibling {
        match current.node_type() {
            "comment" | "line_comment" | "block_comment" | "documentation_comment" => {
                comments.insert(0, get_node_text(current, source));
                sibling = current.previous_named_sibling.as_deref();
            }
            _ => break,
        }
    }

    if comments.is_empty() {
        get_contiguous_docstring_before(anchor, source)
    } else {
        let doc = comments
            .iter()
            .map(|comment| clean_comment_markers(comment))
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_owned();
        (!doc.is_empty()).then_some(doc)
    }
}

fn get_contiguous_docstring_before(node: &SyntaxNode, source: &str) -> Option<String> {
    // 有些 tree-sitter grammar 不把注释作为 named sibling 暴露，只能从源码
    // 文本向上回扫连续注释行。遇到空白后的第一段注释仍视为同一 docstring。
    let before = source.get(..node.start_index)?;
    let mut comments = Vec::new();

    for line in before.lines().rev() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if comments.is_empty() {
                continue;
            }
            break;
        }
        if comments.is_empty() && is_declaration_prefix_line(trimmed) {
            continue;
        }
        if is_doc_comment_line(trimmed) {
            comments.insert(0, trimmed.to_owned());
            continue;
        }
        break;
    }

    if comments.is_empty() {
        None
    } else {
        let doc = comments
            .iter()
            .map(|comment| clean_comment_markers(comment))
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_owned();
        (!doc.is_empty()).then_some(doc)
    }
}

fn is_doc_comment_line(line: &str) -> bool {
    line.starts_with("//")
        || line.starts_with('#')
        || line.starts_with("/*")
        || line.starts_with('*')
        || line.starts_with("--")
        || line.starts_with('{')
        || line.starts_with("(*")
}

fn is_declaration_prefix_line(line: &str) -> bool {
    // 处理 `export const` / `pub async fn` 这类声明前缀跨行时，允许跳过只含
    // 修饰符的行；一旦出现业务标识符就停止，避免误吸收普通注释。
    let normalized = line
        .trim_end_matches('=')
        .trim_end_matches(':')
        .trim_end_matches('(')
        .trim();
    let mut saw_token = false;
    for token in normalized.split_whitespace() {
        saw_token = true;
        if !matches!(
            token,
            "export"
                | "default"
                | "declare"
                | "const"
                | "let"
                | "var"
                | "function"
                | "class"
                | "interface"
                | "struct"
                | "enum"
                | "trait"
                | "pub"
                | "mut"
                | "async"
                | "static"
                | "public"
                | "private"
                | "protected"
                | "readonly"
                | "final"
        ) {
            return false;
        }
    }
    saw_token
}
