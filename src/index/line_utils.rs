//! fallback 文本扫描的行级/范围级工具。
//!
//! 这里的分隔符匹配会跳过字符串并追踪括号深度，专门用于对象字面量、函数参数和 store action 的安全切片。

use super::*;

pub(super) fn distinctive_value_name(name: &str) -> bool {
    name.len() >= 3 && name.chars().any(|ch| ch == '_' || ch.is_ascii_uppercase())
}

pub(super) fn method_decl_keyword(token: &str) -> bool {
    matches!(
        token,
        "async"
            | "static"
            | "public"
            | "private"
            | "protected"
            | "internal"
            | "override"
            | "final"
            | "function"
            | "func"
            | "fun"
            | "def"
            | "fn"
    )
}

pub(super) fn value_decl_keyword(token: &str) -> bool {
    matches!(
        token.trim_start_matches('$'),
        "static"
            | "const"
            | "final"
            | "readonly"
            | "let"
            | "val"
            | "var"
            | "int"
            | "u32"
            | "str"
            | "char"
            | "float"
            | "double"
            | "bool"
            | "boolean"
            | "String"
            | "string"
            | "List"
            | "map"
            | "pub"
    )
}

pub(super) fn facade_store_object_ranges(source: &str) -> Vec<(usize, usize)> {
    // 目标是 Zustand 风格 `export const store = create(() => ({ ... }))`，
    // 只返回 create initializer 中真正返回的对象字面量范围。
    let mut ranges = Vec::new();
    let mut cursor = 0usize;
    while let Some(relative_export_start) = source[cursor..].find("export const") {
        let export_start = cursor + relative_export_start;
        let Some((_, _, name_end)) =
            facade_exported_const_name(source, export_start + "export const".len())
        else {
            cursor = export_start + "export const".len();
            continue;
        };
        let Some(equal_relative) = source[name_end..].find('=') else {
            cursor = name_end;
            continue;
        };
        let initializer_start = name_end + equal_relative + 1;
        let initializer_end = source[initializer_start..]
            .find("export const")
            .map(|next| initializer_start + next)
            .unwrap_or(source.len());
        let initializer = &source[initializer_start..initializer_end];
        if initializer.contains("create")
            && let Some(range) = facade_find_returned_object_in_initializer(
                source,
                initializer_start,
                initializer_end,
            )
        {
            ranges.push(range);
        }
        cursor = initializer_end;
    }
    ranges
}

pub(super) fn facade_exported_const_name(
    source: &str,
    start: usize,
) -> Option<(String, usize, usize)> {
    let name_start = facade_skip_whitespace(source, start, source.len());
    let first = source[name_start..].chars().next()?;
    if !is_identifier_char(first) || first.is_ascii_digit() {
        return None;
    }
    let mut name_end = name_start + first.len_utf8();
    for (relative, ch) in source[name_end..].char_indices() {
        if !is_identifier_char(ch) {
            break;
        }
        name_end = name_start + first.len_utf8() + relative + ch.len_utf8();
    }
    Some((
        source[name_start..name_end].to_owned(),
        name_start,
        name_end,
    ))
}

pub(super) fn facade_find_returned_object_in_initializer(
    source: &str,
    start: usize,
    end: usize,
) -> Option<(usize, usize)> {
    let mut cursor = start;
    while cursor < end {
        let Some(relative_arrow) = source[cursor..end].find("=>") else {
            break;
        };
        let arrow = cursor + relative_arrow;
        let after_arrow = facade_skip_whitespace(source, arrow + 2, end);
        if source.as_bytes().get(after_arrow) == Some(&b'(') {
            let inner = facade_skip_whitespace(source, after_arrow + 1, end);
            if source.as_bytes().get(inner) == Some(&b'{')
                && let Some(object_end) = facade_find_matching_delim(source, inner, '{', '}', end)
            {
                return Some((inner, object_end));
            }
        }
        if source.as_bytes().get(after_arrow) == Some(&b'{')
            && let Some(block_end) = facade_find_matching_delim(source, after_arrow, '{', '}', end)
            && let Some(object) =
                facade_find_return_object_in_block(source, after_arrow + 1, block_end)
        {
            return Some(object);
        }
        cursor = arrow + 2;
    }
    None
}

pub(super) fn facade_find_return_object_in_block(
    source: &str,
    start: usize,
    end: usize,
) -> Option<(usize, usize)> {
    let mut cursor = start;
    while cursor < end {
        let Some(relative_return) = source[cursor..end].find("return") else {
            break;
        };
        let return_start = cursor + relative_return;
        let after_return = facade_skip_whitespace(source, return_start + "return".len(), end);
        let object_start = if source.as_bytes().get(after_return) == Some(&b'(') {
            facade_skip_whitespace(source, after_return + 1, end)
        } else {
            after_return
        };
        if source.as_bytes().get(object_start) == Some(&b'{')
            && let Some(object_end) =
                facade_find_matching_delim(source, object_start, '{', '}', end)
        {
            return Some((object_start, object_end));
        }
        cursor = return_start + "return".len();
    }
    None
}

pub(super) fn facade_find_matching_delim(
    source: &str,
    open_index: usize,
    open: char,
    close: char,
    end: usize,
) -> Option<usize> {
    // 这是 fallback 多处复用的核心扫描器：忽略字符串中的括号，避免对象/参数切片提前截断。
    let mut depth = 0isize;
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for (relative, ch) in source[open_index..end].char_indices() {
        let index = open_index + relative;
        if let Some(quote_char) = quote {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == quote_char {
                quote = None;
            }
            continue;
        }
        if matches!(ch, '\'' | '"' | '`') {
            quote = Some(ch);
            continue;
        }
        if ch == open {
            depth += 1;
        } else if ch == close {
            depth -= 1;
            if depth == 0 {
                return Some(index);
            }
        }
    }
    None
}

pub(super) fn facade_split_top_level_segments(
    source: &str,
    start: usize,
    end: usize,
) -> Vec<(usize, usize)> {
    // 只按顶层逗号切分，保留嵌套函数参数、数组和对象里的逗号。
    let mut segments = Vec::new();
    let mut segment_start = start;
    let mut paren_depth = 0isize;
    let mut brace_depth = 0isize;
    let mut bracket_depth = 0isize;
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for (relative, ch) in source[start..end].char_indices() {
        let index = start + relative;
        if let Some(quote_char) = quote {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == quote_char {
                quote = None;
            }
            continue;
        }
        match ch {
            '\'' | '"' | '`' => quote = Some(ch),
            '(' => paren_depth += 1,
            ')' => paren_depth -= 1,
            '{' => brace_depth += 1,
            '}' => brace_depth -= 1,
            '[' => bracket_depth += 1,
            ']' => bracket_depth -= 1,
            ',' if paren_depth == 0 && brace_depth == 0 && bracket_depth == 0 => {
                segments.push((segment_start, index));
                segment_start = index + ch.len_utf8();
            }
            _ => {}
        }
    }
    segments.push((segment_start, end));
    segments
}

pub(super) fn facade_object_function_property(
    source: &str,
    start: usize,
    end: usize,
) -> Option<(String, usize, usize)> {
    // 支持 `name: () => {}`、`name: function(){}` 和对象方法简写 `name() {}` 三种 action 写法。
    let (trimmed_start, trimmed_end) = facade_trim_range(source, start, end);
    if trimmed_start >= trimmed_end {
        return None;
    }
    if let Some(colon) = facade_find_top_level_char(source, trimmed_start, trimmed_end, ':') {
        let name = facade_strip_object_key(&source[trimmed_start..colon])?;
        let value_start = facade_skip_whitespace(source, colon + 1, trimmed_end);
        let value = source[value_start..trimmed_end].trim_start();
        if value.contains("=>")
            || value.starts_with("function")
            || value.starts_with("async function")
        {
            return Some((name, value_start, trimmed_end));
        }
        return None;
    }

    let paren = facade_find_top_level_char(source, trimmed_start, trimmed_end, '(')?;
    let name = facade_strip_object_key(&source[trimmed_start..paren])?;
    Some((name, paren, trimmed_end))
}

pub(super) fn facade_find_top_level_char(
    source: &str,
    start: usize,
    end: usize,
    target: char,
) -> Option<usize> {
    let mut paren_depth = 0isize;
    let mut brace_depth = 0isize;
    let mut bracket_depth = 0isize;
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for (relative, ch) in source[start..end].char_indices() {
        let index = start + relative;
        if let Some(quote_char) = quote {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == quote_char {
                quote = None;
            }
            continue;
        }
        match ch {
            '\'' | '"' | '`' => quote = Some(ch),
            '(' => paren_depth += 1,
            ')' => paren_depth -= 1,
            '{' => brace_depth += 1,
            '}' => brace_depth -= 1,
            '[' => bracket_depth += 1,
            ']' => bracket_depth -= 1,
            _ if ch == target && paren_depth == 0 && brace_depth == 0 && bracket_depth == 0 => {
                return Some(index);
            }
            _ => {}
        }
    }
    None
}

pub(super) fn facade_strip_object_key(raw: &str) -> Option<String> {
    let key = raw.trim().trim_matches(['"', '\'', '`']).trim().to_owned();
    (!key.is_empty()
        && key
            .chars()
            .next()
            .is_some_and(|ch| is_identifier_char(ch) && !ch.is_ascii_digit()))
    .then_some(key)
}

pub(super) fn facade_trim_range(source: &str, start: usize, end: usize) -> (usize, usize) {
    let mut trimmed_start = start;
    while trimmed_start < end
        && source[trimmed_start..]
            .chars()
            .next()
            .is_some_and(char::is_whitespace)
    {
        trimmed_start += source[trimmed_start..]
            .chars()
            .next()
            .map(char::len_utf8)
            .unwrap_or(1);
    }
    let mut trimmed_end = end;
    while trimmed_end > trimmed_start
        && source[..trimmed_end]
            .chars()
            .next_back()
            .is_some_and(char::is_whitespace)
    {
        trimmed_end -= source[..trimmed_end]
            .chars()
            .next_back()
            .map(char::len_utf8)
            .unwrap_or(1);
    }
    (trimmed_start, trimmed_end)
}

pub(super) fn facade_skip_whitespace(source: &str, start: usize, end: usize) -> usize {
    let mut cursor = start;
    while cursor < end {
        let Some(ch) = source[cursor..end].chars().next() else {
            break;
        };
        if !ch.is_whitespace() {
            break;
        }
        cursor += ch.len_utf8();
    }
    cursor
}

pub(super) fn facade_line_number_at(source: &str, index: usize) -> u64 {
    source[..index.min(source.len())]
        .chars()
        .filter(|ch| *ch == '\n')
        .count() as u64
        + 1
}
