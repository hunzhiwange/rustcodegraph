use super::*;

// 多个语言特例共享的小工具集中在这里，保持主抽取流程可读；函数应保持纯文本/
// 纯 AST 查询，不在这里写入节点或边。
pub(super) fn is_word_identifier(input: &str) -> bool {
    let mut chars = input.chars();
    matches!(chars.next(), Some('_') | Some('A'..='Z') | Some('a'..='z'))
        && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

pub(super) fn is_pascal_type_prefix(input: &str) -> bool {
    let mut chars = input.chars();
    matches!(chars.next(), Some('T') | Some('I'))
        && chars.next().is_some_and(|ch| ch.is_ascii_uppercase())
}

pub(super) fn pascal_identifier_texts(node: &SyntaxNode, source: &str) -> Vec<String> {
    node.named_children
        .iter()
        .filter(|child| child.node_type() == "identifier")
        .map(|child| get_node_text(child, source))
        .collect()
}

pub(super) fn pascal_decl_section_visibility(node: &SyntaxNode) -> Option<Visibility> {
    for child in &node.children {
        match child.node_type() {
            "kPublic" | "kPublished" => return Some(Visibility::Public),
            "kPrivate" => return Some(Visibility::Private),
            "kProtected" => return Some(Visibility::Protected),
            _ => {}
        }
    }
    None
}

pub(super) fn simple_php_type_name(input: &str) -> String {
    input
        .trim()
        .trim_start_matches('\\')
        .rsplit('\\')
        .find(|part| !part.is_empty())
        .unwrap_or(input)
        .trim()
        .to_owned()
}

pub(super) fn strip_balanced_suffix(input: &str, open: char, close: char) -> String {
    let mut out = String::new();
    let mut depth = 0usize;
    for ch in input.chars() {
        if ch == open {
            depth += 1;
            continue;
        }
        if ch == close {
            depth = depth.saturating_sub(1);
            continue;
        }
        if depth == 0 {
            out.push(ch);
        }
    }
    out
}

pub(super) fn collect_descendants_of_type(node: &SyntaxNode, wanted: &str) -> Vec<SyntaxNode> {
    collect_descendants_matching(node, &[wanted])
}

pub(super) fn collect_descendants_matching(node: &SyntaxNode, wanted: &[&str]) -> Vec<SyntaxNode> {
    let mut found = Vec::new();
    let mut stack = node.named_children.clone();
    while let Some(current) = stack.pop() {
        if wanted.contains(&current.node_type()) {
            found.push(current.clone());
        }
        stack.extend(current.named_children.iter().cloned());
    }
    found.reverse();
    found
}
