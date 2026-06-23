use super::*;

// TypeScript 类型文本相关工具只做保守识别：抽 PascalCase 类型名、tuple contract
// 入口和展示用签名折叠，不尝试完整解析类型系统。
pub(super) fn ts_js_type_identifier_names(input: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut start = None;
    for (idx, ch) in input.char_indices() {
        if start.is_none() {
            if is_ident_start(ch) {
                start = Some(idx);
            }
        } else if !is_ident_continue(ch)
            && let Some(token_start) = start.take()
        {
            let token = &input[token_start..idx];
            if token
                .chars()
                .next()
                .is_some_and(|first| first.is_ascii_uppercase())
            {
                names.push(token.to_owned());
            }
        }
    }
    if let Some(token_start) = start {
        let token = &input[token_start..];
        if token
            .chars()
            .next()
            .is_some_and(|first| first.is_ascii_uppercase())
        {
            names.push(token.to_owned());
        }
    }
    names
}

pub(super) fn collect_ts_tuple_types<'a>(
    node: &'a SyntaxNode,
    depth: usize,
    tuples: &mut Vec<&'a SyntaxNode>,
) {
    if depth > 6 {
        return;
    }
    if node.node_type() == "tuple_type" {
        tuples.push(node);
    }
    for child in &node.named_children {
        collect_ts_tuple_types(child, depth + 1, tuples);
    }
}

pub(super) fn ts_tuple_direct_generic_entry(entry: &SyntaxNode) -> Option<&SyntaxNode> {
    if entry.node_type() == "generic_type" {
        return Some(entry);
    }
    if !matches!(
        entry.node_type(),
        "required_parameter" | "optional_parameter" | "optional_type" | "rest_type"
    ) {
        return None;
    }
    entry
        .named_children
        .iter()
        .find(|child| child.node_type() == "generic_type")
}

pub(super) fn is_valid_ts_contract_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == '_' || first == '$') {
        return false;
    }
    chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '$')
}

pub(super) fn collapse_whitespace(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(super) fn simple_type_name(input: &str) -> String {
    let without_generics = strip_balanced_suffix(input.trim(), '<', '>');
    let without_args = without_generics
        .split('(')
        .next()
        .unwrap_or(&without_generics);
    without_args
        .rsplit(['.', ':'])
        .find(|part| !part.is_empty())
        .unwrap_or(without_args)
        .trim()
        .to_owned()
}
