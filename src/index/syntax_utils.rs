//! fallback 抽取共享的低层语法工具。
//!
//! 这些函数只处理跨语言共同的标识符、括号深度和简单调用形态；语言特定规则放在相邻模块中。

use super::*;

pub(super) fn strip_visibility(input: &str) -> (Option<Visibility>, &str) {
    for (prefix, visibility) in [
        ("public ", Visibility::Public),
        ("private ", Visibility::Private),
        ("protected ", Visibility::Protected),
    ] {
        if let Some(rest) = input.strip_prefix(prefix) {
            return (Some(visibility), rest.trim_start());
        }
    }
    (None, input)
}

pub(super) fn strip_static(input: &str) -> (bool, &str) {
    input
        .strip_prefix("static ")
        .map(|rest| (true, rest.trim_start()))
        .unwrap_or((false, input))
}

pub(super) fn looks_like_method_declaration(input: &str) -> bool {
    let Some(paren) = input.find('(') else {
        return false;
    };
    let eq = input.find('=').unwrap_or(usize::MAX);
    let colon = input.find(':').unwrap_or(usize::MAX);
    paren < eq && paren < colon
}

pub(super) fn first_identifier(input: &str) -> Option<&str> {
    let input = input.trim_start();
    let end = input
        .char_indices()
        .find_map(|(idx, ch)| (!is_identifier_char(ch)).then_some(idx))
        .unwrap_or(input.len());
    (end > 0).then(|| &input[..end])
}

pub(super) fn trim_identifier(input: &str) -> &str {
    input.trim_matches(|ch: char| !is_identifier_char(ch))
}

pub(super) fn is_identifier_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_' || ch == '$'
}

pub(super) fn brace_delta(input: &str) -> isize {
    input.chars().filter(|ch| *ch == '{').count() as isize
        - input.chars().filter(|ch| *ch == '}').count() as isize
}

pub(super) fn block_end_line(lines: &[&str], start_idx: usize) -> u64 {
    // 花括号语言用深度归零判断 block 结束；找不到闭合时保守返回声明行，避免跨文件吞噬。
    let mut depth = 0isize;
    let mut saw_open_brace = false;

    for (idx, line) in lines.iter().enumerate().skip(start_idx) {
        if line.contains('{') {
            saw_open_brace = true;
        }
        depth += brace_delta(line);
        if saw_open_brace && depth <= 0 {
            return (idx + 1) as u64;
        }
    }

    (start_idx + 1) as u64
}

pub(super) fn pascal_function_end_line(lines: &[&str], start_idx: usize) -> u64 {
    for (idx, line) in lines.iter().enumerate().skip(start_idx + 1) {
        if line.trim().eq_ignore_ascii_case("end;") {
            return (idx + 1) as u64;
        }
    }
    (start_idx + 1) as u64
}

pub(super) fn pascal_has_body(lines: &[&str], start_idx: usize) -> bool {
    for line in lines.iter().skip(start_idx + 1) {
        let trimmed = line.trim().to_ascii_lowercase();
        if trimmed == "begin" || trimmed.starts_with("begin ") {
            return true;
        }
        if trimmed.starts_with("procedure ")
            || trimmed.starts_with("function ")
            || trimmed.starts_with("constructor ")
            || trimmed.starts_with("destructor ")
            || trimmed == "implementation"
            || trimmed == "end."
        {
            return false;
        }
    }
    false
}

pub(super) fn ruby_function_end_line(lines: &[&str], start_idx: usize) -> u64 {
    for (idx, line) in lines.iter().enumerate().skip(start_idx + 1) {
        if line.trim() == "end" {
            return (idx + 1) as u64;
        }
    }
    (start_idx + 1) as u64
}

pub(super) fn python_function_end_line(lines: &[&str], start_idx: usize) -> u64 {
    let header_indent = leading_indent_width(lines.get(start_idx).copied().unwrap_or_default());
    let mut end_line = (start_idx + 1) as u64;

    for (idx, line) in lines.iter().enumerate().skip(start_idx + 1) {
        if line.trim().is_empty() {
            continue;
        }
        if leading_indent_width(line) <= header_indent {
            break;
        }
        end_line = (idx + 1) as u64;
    }

    end_line
}

pub(super) fn leading_indent_width(line: &str) -> usize {
    line.chars()
        .take_while(|ch| matches!(ch, ' ' | '\t'))
        .map(|ch| if ch == '\t' { 4 } else { 1 })
        .sum()
}

pub(super) fn type_identifiers(input: &str) -> Vec<String> {
    identifier_tokens(input)
        .into_iter()
        .filter(|token| {
            token
                .chars()
                .next()
                .map(|ch| ch.is_ascii_uppercase())
                .unwrap_or(false)
        })
        .collect()
}

pub(super) fn identifier_tokens(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut start = None;
    for (idx, ch) in input.char_indices() {
        if is_identifier_char(ch) {
            start.get_or_insert(idx);
        } else if let Some(token_start) = start.take() {
            tokens.push(input[token_start..idx].to_owned());
        }
    }
    if let Some(token_start) = start {
        tokens.push(input[token_start..].to_owned());
    }
    tokens
}

pub(super) fn call_names(input: &str) -> Vec<String> {
    let mut names = Vec::new();
    for (idx, _) in input.match_indices("this.") {
        let after_this = idx + "this.".len();
        if let Some(name) = first_identifier(&input[after_this..]) {
            let after_name = after_this + name.len();
            if input[after_name..].trim_start().starts_with('(') {
                names.push(name.to_owned());
            }
        }
    }
    names
}

pub(super) fn member_call_names(input: &str) -> Vec<String> {
    // 捕获 `receiver.method()` 和 `receiver->method()`，保留 receiver 文本供成员解析推断字段类型。
    let mut names = Vec::new();
    let mut seen = HashSet::new();
    for (idx, ch) in input.char_indices() {
        if ch != '(' {
            continue;
        }
        let before = input[..idx].trim_end();
        let member_end = before.len();
        let member_start = before
            .char_indices()
            .rev()
            .find_map(|(idx, ch)| (!is_identifier_char(ch)).then_some(idx + ch.len_utf8()))
            .unwrap_or(0);
        if member_start >= member_end {
            continue;
        }
        let member = &before[member_start..member_end];
        if is_call_keyword(member) {
            continue;
        }
        let receiver_part = before[..member_start].trim_end();
        let receiver = if let Some(receiver) = receiver_part.strip_suffix('.') {
            receiver.trim_end()
        } else if let Some(receiver) = receiver_part.strip_suffix("->") {
            receiver.trim_end()
        } else {
            continue;
        };
        if receiver.is_empty() || receiver.ends_with("new") {
            continue;
        }
        let receiver = receiver
            .rsplit(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_' || ch == '$' || ch == '.'))
            .next()
            .unwrap_or(receiver)
            .trim();
        if receiver.is_empty() {
            continue;
        }
        let name = format!("{receiver}.{member}");
        if seen.insert(name.clone()) {
            names.push(name);
        }
    }
    names
}

pub(super) fn bare_call_names(input: &str) -> Vec<String> {
    // fallback 的裸调用扫描不理解作用域，只提取候选名，后续 resolver 再按语言和同文件规则过滤。
    let mut names = Vec::new();
    for (idx, ch) in input.char_indices() {
        if ch != '(' {
            continue;
        }
        let before = input[..idx].trim_end();
        let end = before.len();
        let start = before
            .char_indices()
            .rev()
            .find_map(|(idx, ch)| (!is_identifier_char(ch)).then_some(idx + ch.len_utf8()))
            .unwrap_or(0);
        if start >= end {
            continue;
        }
        let name = &before[start..end];
        if is_call_keyword(name) {
            continue;
        }
        names.push(name.to_owned());
    }
    names
}

pub(super) fn new_expression_names(input: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut rest = input;
    while let Some(idx) = rest.find("new ") {
        let after_new = &rest[idx + 4..];
        if let Some(name) = first_identifier(after_new) {
            names.push(name.to_owned());
            rest = &after_new[name.len()..];
        } else {
            rest = after_new;
        }
    }
    names
}

pub(super) fn is_call_keyword(name: &str) -> bool {
    matches!(
        name,
        "if" | "for"
            | "while"
            | "switch"
            | "catch"
            | "function"
            | "return"
            | "typeof"
            | "sizeof"
            | "new"
    )
}

pub(super) fn facade_is_test_file(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.contains("__tests__/")
        || lower.contains("/tests/")
        || lower.contains("/test/")
        || lower.contains(".test.")
        || lower.contains(".spec.")
        || lower.ends_with("_test.rs")
        || lower.ends_with("_test.go")
        || lower.ends_with("test.py")
}

pub(super) fn sanitize_id_part(input: &str) -> String {
    input
        .chars()
        .map(|ch| if is_identifier_char(ch) { ch } else { '_' })
        .collect()
}
