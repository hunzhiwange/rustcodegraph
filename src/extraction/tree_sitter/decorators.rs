use super::*;

// decorator/annotation 在不同 grammar 中可能是专门节点、`@` 前缀的 call，
// 或直接拼在声明文本里；这些 helper 把它们统一成 Decorates 引用名。
pub(super) fn is_decorator_like_node(node: &SyntaxNode) -> bool {
    matches!(
        node.node_type(),
        "decorator" | "annotation" | "marker_annotation" | "attribute"
    )
}

pub(super) fn is_at_prefixed_node(node: &SyntaxNode, source: &str) -> bool {
    if node.node_type() != "call_expression" {
        return false;
    }
    source[..node.start_index]
        .chars()
        .rev()
        .find(|ch| !ch.is_whitespace())
        == Some('@')
}

pub(super) fn decorator_reference_name(
    node: &SyntaxNode,
    source: &str,
) -> Option<(String, usize, usize)> {
    let target = decorator_target_node(node)?;
    let name = simple_type_name(&get_node_text(target, source));
    (!name.is_empty()).then_some((
        name,
        node.start_position.row + 1,
        node.start_position.column,
    ))
}

pub(super) fn decorator_target_node(node: &SyntaxNode) -> Option<&SyntaxNode> {
    for child in &node.named_children {
        if child.node_type() == "call_expression"
            && let Some(function) =
                get_child_by_field(child, "function").or_else(|| child.named_child(0))
        {
            return Some(function);
        }
        if matches!(
            child.node_type(),
            "identifier"
                | "member_expression"
                | "scoped_identifier"
                | "navigation_expression"
                | "user_type"
                | "type_identifier"
        ) {
            return Some(child);
        }
    }
    None
}

pub(super) fn decorator_names_from_decl_prefix(text: &str) -> Vec<(String, usize)> {
    // 处理装饰器被包含在声明节点文本开头的 grammar，line_offset 用于还原位置。
    let mut names = Vec::new();
    for (line_offset, line) in text.lines().enumerate() {
        let trimmed = line.trim_start();
        if !trimmed.starts_with('@') {
            if trimmed.is_empty() {
                continue;
            }
            break;
        }
        let after_at = &trimmed[1..];
        let end = after_at
            .char_indices()
            .find(|(_, ch)| !(ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '$' || *ch == '.'))
            .map(|(idx, _)| idx)
            .unwrap_or(after_at.len());
        let name = simple_type_name(&after_at[..end]);
        if !name.is_empty() {
            names.push((name, line_offset));
        }
    }
    names
}

pub(super) fn decorator_names_before_node(node: &SyntaxNode, source: &str) -> Vec<(String, usize)> {
    let before = &source[..node.start_index];
    let line = before.rsplit('\n').next().unwrap_or_default();
    let Some(at_idx) = line.find('@') else {
        return Vec::new();
    };
    let trimmed = line[at_idx + 1..].trim_start();
    let end = trimmed
        .char_indices()
        .find(|(_, ch)| !(ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '$' || *ch == '.'))
        .map(|(idx, _)| idx)
        .unwrap_or(trimmed.len());
    let name = simple_type_name(&trimmed[..end]);
    if name.is_empty() {
        Vec::new()
    } else {
        vec![(name, at_idx)]
    }
}
