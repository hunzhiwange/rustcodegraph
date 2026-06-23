//! 函数引用表达式的轻量解析工具。
//!
//! 这些函数只识别高置信度的 callback/函数指针候选，宁可漏掉复杂表达式，也避免把普通变量误连成函数边。

use super::*;

pub(super) fn facade_static_initializer_chunks(source: &str) -> Vec<(u64, String)> {
    let mut chunks = Vec::new();
    let lines = source.lines().collect::<Vec<_>>();
    let mut idx = 0usize;
    while idx < lines.len() {
        let trimmed = lines[idx].trim();
        if !(trimmed.starts_with("static ") && trimmed.contains('=') && trimmed.contains('{')) {
            idx += 1;
            continue;
        }
        let start_line = (idx + 1) as u64;
        let mut chunk = String::new();
        while idx < lines.len() {
            chunk.push_str(lines[idx]);
            chunk.push('\n');
            if lines[idx].contains("};") {
                break;
            }
            idx += 1;
        }
        chunks.push((start_line, chunk));
        idx += 1;
    }
    chunks
}

pub(super) fn facade_candidate_from_expr(expr: &str, language: Language) -> Option<String> {
    let mut value = expr
        .trim()
        .trim_end_matches(';')
        .trim()
        .trim_matches(|ch| ch == '(' || ch == ')')
        .trim();
    if let Some((before_as, _)) = value.split_once(" as ") {
        value = before_as.trim();
    }
    value = value.split([';', '}', ']']).next().unwrap_or(value).trim();
    value = value.trim_start_matches('&').trim_start_matches('@').trim();
    if let Some(member) = value.strip_prefix("this.") {
        // `this.foo` 统一规范化为 this 成员引用，后续 resolver 再按当前类/父类解析。
        return first_identifier(member).map(|name| format!("this.{}", trim_identifier(name)));
    }
    if let Some(member) = value
        .strip_prefix("this::")
        .or_else(|| value.strip_prefix("super::"))
        .or_else(|| value.strip_prefix("self::"))
        .or_else(|| value.strip_prefix("super."))
        .or_else(|| value.strip_prefix("self."))
    {
        return first_identifier(member).map(|name| format!("this.{}", trim_identifier(name)));
    }
    if let Some((receiver, member_tail)) = value.split_once("::") {
        let member = first_identifier(member_tail).map(trim_identifier)?;
        let receiver = receiver.trim().trim_start_matches('&').trim();
        if receiver
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_uppercase())
            || matches!(
                language,
                Language::Cpp | Language::Java | Language::Kotlin | Language::Php
            )
        {
            if receiver
                .chars()
                .next()
                .is_some_and(|ch| ch.is_ascii_lowercase())
            {
                // 小写 receiver 更可能是变量作用域而不是类型名，保守丢弃以降低误连。
                return None;
            }
            return Some(format!("{receiver}::{member}"));
        }
    }
    if is_simple_identifier(value) {
        return Some(value.to_owned());
    }
    None
}

pub(super) fn facade_assignment_rhs(line: &str) -> Option<&str> {
    if let Some((_, rhs)) = line.split_once(":=") {
        return Some(rhs);
    }
    for (idx, ch) in line.char_indices() {
        if ch != '=' {
            continue;
        }
        let prev = line[..idx].chars().next_back();
        let next = line[idx + ch.len_utf8()..].chars().next();
        if matches!(prev, Some('=' | '!' | '<' | '>' | '-')) || matches!(next, Some('=' | '>')) {
            // 跳过比较、箭头和复合运算符；这里只有赋值右侧才可能是函数值引用。
            continue;
        }
        return Some(&line[idx + ch.len_utf8()..]);
    }
    None
}

pub(super) fn facade_expr_is_explicit_fn_ref(expr: &str) -> bool {
    let trimmed = expr.trim();
    trimmed.starts_with('&')
        || trimmed.starts_with('@')
        || trimmed.starts_with("this.")
        || trimmed.starts_with("this::")
        || trimmed.starts_with("super::")
        || trimmed.contains("::")
}

pub(super) fn facade_callee_before_paren(before: &str) -> Option<String> {
    identifier_tokens(before).into_iter().last()
}

pub(super) fn quoted_string_content(input: &str) -> Option<String> {
    let trimmed = input.trim();
    let quote = trimmed.chars().next()?;
    if !matches!(quote, '\'' | '"') || !trimmed.ends_with(quote) || trimmed.len() < 2 {
        return None;
    }
    Some(trimmed[1..trimmed.len() - 1].to_owned())
}

pub(super) fn strip_quoted_segments(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for ch in input.chars() {
        if let Some(quote_char) = quote {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == quote_char {
                quote = None;
            }
            out.push(' ');
            continue;
        }
        if matches!(ch, '\'' | '"') {
            quote = Some(ch);
            out.push(' ');
        } else {
            out.push(ch);
        }
    }
    out
}

pub(super) fn is_simple_identifier(input: &str) -> bool {
    let mut chars = input.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_' || first == '$')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '$')
}

pub(super) fn facade_fn_ref_stop_name(name: &str) -> bool {
    matches!(
        name,
        "this"
            | "self"
            | "super"
            | "null"
            | "nil"
            | "true"
            | "false"
            | "undefined"
            | "new"
            | "NULL"
            | "nullptr"
            | "None"
    )
}

pub(super) fn dedupe_names(names: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    names
        .into_iter()
        .filter(|name| !facade_fn_ref_stop_name(name) && seen.insert(name.clone()))
        .collect()
}

pub(super) const PHP_CALLABLE_HOFS: &[&str] = &[
    // PHP 标准库中会接收 callable 的高阶函数名，字符串 callback 只在这些上下文里提升为函数引用。
    "array_map",
    "array_filter",
    "array_walk",
    "array_walk_recursive",
    "array_reduce",
    "usort",
    "uasort",
    "uksort",
    "array_udiff",
    "array_udiff_assoc",
    "array_uintersect",
    "array_uintersect_assoc",
    "call_user_func",
    "call_user_func_array",
    "forward_static_call",
    "forward_static_call_array",
    "preg_replace_callback",
    "preg_replace_callback_array",
    "register_shutdown_function",
    "register_tick_function",
    "set_error_handler",
    "set_exception_handler",
    "spl_autoload_register",
    "ob_start",
    "iterator_apply",
    "header_register_callback",
    "is_callable",
];

pub(super) fn push_facade_executable_edges(
    pending_edges: &mut Vec<RichFacadePendingEdge>,
    source: &str,
    code: &str,
    owner_name: Option<&str>,
    line_number: u64,
) {
    // fallback 没有 AST，可执行边只从显式 new 和裸调用提取；owner_name 过滤掉构造/递归式自引用噪音。
    for target_name in new_expression_names(code) {
        if Some(target_name.as_str()) == owner_name {
            continue;
        }
        pending_edges.push(RichFacadePendingEdge {
            source: source.to_owned(),
            target_name,
            kind: EdgeKind::Instantiates,
            metadata: None,
            line: Some(line_number),
            column: Some(0),
        });
    }

    for target_name in bare_call_names(code) {
        if Some(target_name.as_str()) == owner_name {
            continue;
        }
        pending_edges.push(RichFacadePendingEdge {
            source: source.to_owned(),
            target_name,
            kind: EdgeKind::Calls,
            metadata: None,
            line: Some(line_number),
            column: Some(0),
        });
    }
}

pub(super) fn facade_contains_edge(source: &str, target: &str, line_number: u64) -> Edge {
    Edge {
        source: source.to_owned(),
        target: target.to_owned(),
        kind: EdgeKind::Contains,
        metadata: None,
        line: Some(line_number),
        column: Some(0),
        provenance: None,
    }
}
