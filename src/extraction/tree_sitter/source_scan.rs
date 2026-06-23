// 这些文本扫描 helper 服务于受限 fallback，不是通用 JS parser。它们都尽量只在
// 顶层分隔符上切分，并显式跳过字符串，避免在启发式补洞时制造大量错误边。

pub(super) fn exported_const_name(source: &str, start: usize) -> Option<(String, usize, usize)> {
    let name_start = skip_whitespace(source, start, source.len());
    let first = source[name_start..].chars().next()?;
    if !is_ident_start(first) {
        return None;
    }
    let name_end = ident_end_at(source, name_start);
    Some((
        source[name_start..name_end].to_owned(),
        name_start,
        name_end,
    ))
}

pub(super) fn find_returned_object_in_initializer(
    source: &str,
    start: usize,
    end: usize,
) -> Option<(usize, usize)> {
    // 支持 `() => ({ ... })` 和 `() => { return { ... } }` 两种 store 常见写法。
    let mut cursor = start;
    while cursor < end {
        let Some(relative_arrow) = source[cursor..end].find("=>") else {
            break;
        };
        let arrow = cursor + relative_arrow;
        let after_arrow = skip_whitespace(source, arrow + 2, end);
        if source.as_bytes().get(after_arrow) == Some(&b'(') {
            let inner = skip_whitespace(source, after_arrow + 1, end);
            if source.as_bytes().get(inner) == Some(&b'{')
                && let Some(object_end) = find_matching_delim(source, inner, '{', '}', end)
            {
                return Some((inner, object_end));
            }
        }
        if source.as_bytes().get(after_arrow) == Some(&b'{')
            && let Some(block_end) = find_matching_delim(source, after_arrow, '{', '}', end)
            && let Some(object) = find_return_object_in_block(source, after_arrow + 1, block_end)
        {
            return Some(object);
        }
        cursor = arrow + 2;
    }
    None
}

pub(super) fn find_return_object_in_block(
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
        let after_return = skip_whitespace(source, return_start + "return".len(), end);
        let object_start = if source.as_bytes().get(after_return) == Some(&b'(') {
            skip_whitespace(source, after_return + 1, end)
        } else {
            after_return
        };
        if source.as_bytes().get(object_start) == Some(&b'{')
            && let Some(object_end) = find_matching_delim(source, object_start, '{', '}', end)
        {
            return Some((object_start, object_end));
        }
        cursor = return_start + "return".len();
    }
    None
}

pub(super) fn find_matching_delim(
    source: &str,
    open_index: usize,
    open: char,
    close: char,
    end: usize,
) -> Option<usize> {
    // 手写配对只识别 delimiter/quote/escape；不尝试理解注释或正则字面量，
    // 因为调用点已被限制在 store object fallback 的小范围内。
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

pub(super) fn split_top_level_segments(
    source: &str,
    start: usize,
    end: usize,
) -> Vec<(usize, usize)> {
    // object 成员按顶层逗号切分；括号/花括号/方括号内部的逗号必须保留。
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

pub(super) fn object_function_property(
    source: &str,
    start: usize,
    end: usize,
) -> Option<(String, usize, usize)> {
    // 只接受明确的函数值或方法简写，普通配置字段不提升为 Function 节点。
    let (trimmed_start, trimmed_end) = trim_range(source, start, end);
    if trimmed_start >= trimmed_end {
        return None;
    }
    if let Some(colon) = find_top_level_char(source, trimmed_start, trimmed_end, ':') {
        let name = strip_object_key(&source[trimmed_start..colon])?;
        let value_start = skip_whitespace(source, colon + 1, trimmed_end);
        let value = source[value_start..trimmed_end].trim_start();
        if value.contains("=>")
            || value.starts_with("function")
            || value.starts_with("async function")
        {
            return Some((name, value_start, trimmed_end));
        }
        return None;
    }

    let paren = find_top_level_char(source, trimmed_start, trimmed_end, '(')?;
    let name = strip_object_key(&source[trimmed_start..paren])?;
    Some((name, paren, trimmed_end))
}

pub(super) fn find_top_level_char(
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

pub(super) fn extract_call_reference_names(value: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut cursor = 0usize;
    while cursor < value.len() {
        let Some((relative_start, _first)) = value[cursor..]
            .char_indices()
            .find(|(_, ch)| is_ident_start(*ch))
        else {
            break;
        };
        let start = cursor + relative_start;
        let end = ident_end_at(value, start);
        let after = skip_whitespace(value, end, value.len());
        if value.as_bytes().get(after) == Some(&b'(') {
            let name = &value[start..end];
            if !matches!(
                name,
                "if" | "for" | "while" | "switch" | "function" | "return" | "async"
            ) && !names.iter().any(|existing| existing == name)
            {
                names.push(name.to_owned());
            }
        }
        cursor = end.max(start + 1);
    }
    names
}

pub(super) fn strip_object_key(raw: &str) -> Option<String> {
    let key = raw.trim().trim_matches(['"', '\'', '`']).trim().to_owned();
    (!key.is_empty() && key.chars().next().is_some_and(is_ident_start)).then_some(key)
}

pub(super) fn trim_range(source: &str, start: usize, end: usize) -> (usize, usize) {
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

pub(super) fn skip_whitespace(source: &str, start: usize, end: usize) -> usize {
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

pub(super) fn line_col_at(source: &str, index: usize) -> (u64, u64) {
    let mut line = 1u64;
    let mut column = 0u64;
    for ch in source[..index.min(source.len())].chars() {
        if ch == '\n' {
            line += 1;
            column = 0;
        } else {
            column += 1;
        }
    }
    (line, column)
}

pub(super) fn is_ident_start(ch: char) -> bool {
    ch == '_' || ch == '$' || ch.is_ascii_alphabetic()
}

pub(super) fn is_ident_continue(ch: char) -> bool {
    is_ident_start(ch) || ch.is_ascii_digit()
}

pub(super) fn ident_end_at(source: &str, start: usize) -> usize {
    let Some(first) = source[start..].chars().next() else {
        return start;
    };
    let mut end = start + first.len_utf8();
    for (relative, ch) in source[end..].char_indices() {
        if !is_ident_continue(ch) {
            break;
        }
        end = start + first.len_utf8() + relative + ch.len_utf8();
    }
    end
}
