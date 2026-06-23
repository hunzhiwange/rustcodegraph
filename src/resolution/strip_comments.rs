//! Per-language comment stripper for regex-based framework helpers.
//!
//! 中文维护提示：框架 detector 依赖正则扫描源码；剥离注释时必须保留换行位置，
//! 这样后续 line/column 推断不会因为删除文本而整体偏移。

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CommentLang {
    Python,
    JavaScript,
    TypeScript,
    Php,
    Ruby,
    Java,
    CSharp,
    Swift,
    Go,
    Rust,
}

pub fn strip_comments_for_regex(content: &str, lang: CommentLang) -> String {
    match lang {
        CommentLang::Python => strip_python(content),
        CommentLang::Ruby => strip_ruby(content),
        CommentLang::Rust => strip_rust(content),
        CommentLang::Php => strip_php(content),
        CommentLang::Go => strip_go(content),
        CommentLang::JavaScript
        | CommentLang::TypeScript
        | CommentLang::Java
        | CommentLang::CSharp
        | CommentLang::Swift => strip_c_style(
            content,
            matches!(lang, CommentLang::JavaScript | CommentLang::TypeScript),
        ),
    }
}

fn chars(src: &str) -> Vec<char> {
    src.chars().collect()
}

fn blank_range(out: &mut [char], start: usize, end: usize, src: &[char]) {
    // 用空格替换注释而不是删除字符，保留 byte/line 大致位置；换行必须原样留下。
    for i in start..end.min(out.len()) {
        out[i] = if src[i] == '\n' { '\n' } else { ' ' };
    }
}

fn collect(out: Vec<char>) -> String {
    out.into_iter().collect()
}

fn strip_python(src: &str) -> String {
    // Python 三引号常被当成 docstring，也可能出现在正则扫描范围内；这里按注释级
    // 内容清空，避免 decorator/route 正则误读字符串里的示例代码。
    let src = chars(src);
    let mut out = src.clone();
    let mut i = 0;
    while i < src.len() {
        let c = src[i];
        let c2 = src.get(i + 1).copied().unwrap_or('\0');
        let c3 = src.get(i + 2).copied().unwrap_or('\0');
        if (c == '"' || c == '\'') && c2 == c && c3 == c {
            let quote = c;
            let start = i;
            i += 3;
            while i < src.len() {
                if src[i] == '\\' && i + 1 < src.len() {
                    i += 2;
                } else if src[i] == quote
                    && src.get(i + 1) == Some(&quote)
                    && src.get(i + 2) == Some(&quote)
                {
                    i += 3;
                    break;
                } else {
                    i += 1;
                }
            }
            blank_range(&mut out, start, i, &src);
            continue;
        }
        if c == '"' || c == '\'' {
            let quote = c;
            i += 1;
            while i < src.len() && src[i] != quote {
                if src[i] == '\\' && i + 1 < src.len() {
                    i += 2;
                } else if src[i] == '\n' {
                    break;
                } else {
                    i += 1;
                }
            }
            if i < src.len() && src[i] == quote {
                i += 1;
            }
            continue;
        }
        if c == '#' {
            let start = i;
            while i < src.len() && src[i] != '\n' {
                i += 1;
            }
            blank_range(&mut out, start, i, &src);
            continue;
        }
        i += 1;
    }
    collect(out)
}

fn strip_ruby(src: &str) -> String {
    // Ruby 的 `=begin`/`=end` 只有在行首语义成立；at_line_start 用来避免误删普通
    // 字符串或表达式中的 `=begin`。
    let src = chars(src);
    let mut out = src.clone();
    let mut i = 0;
    let mut at_line_start = true;
    while i < src.len() {
        let c = src[i];
        if at_line_start && c == '=' && starts_with(&src, i, "=begin") {
            let start = i;
            i += 6;
            while i < src.len() {
                if src[i] == '\n' {
                    let mut j = i + 1;
                    while j < src.len() && (src[j] == ' ' || src[j] == '\t') {
                        j += 1;
                    }
                    if starts_with(&src, j, "=end") {
                        i = j + 4;
                        while i < src.len() && src[i] != '\n' {
                            i += 1;
                        }
                        break;
                    }
                }
                i += 1;
            }
            blank_range(&mut out, start, i, &src);
            at_line_start = i > 0 && src.get(i - 1) == Some(&'\n');
            continue;
        }
        if c == '"' || c == '\'' {
            let quote = c;
            i += 1;
            while i < src.len() && src[i] != quote {
                if src[i] == '\\' && i + 1 < src.len() {
                    i += 2;
                } else if src[i] == '\n' {
                    break;
                } else {
                    i += 1;
                }
            }
            if i < src.len() && src[i] == quote {
                i += 1;
            }
            at_line_start = false;
            continue;
        }
        if c == '#' {
            let start = i;
            while i < src.len() && src[i] != '\n' {
                i += 1;
            }
            blank_range(&mut out, start, i, &src);
            at_line_start = false;
            continue;
        }
        match c {
            '\n' => {
                at_line_start = true;
                i += 1;
            }
            ' ' | '\t' => i += 1,
            _ => {
                at_line_start = false;
                i += 1;
            }
        }
    }
    collect(out)
}

fn strip_c_style(src: &str, allow_single_quote_strings: bool) -> String {
    strip_slash_comments_and_strings(src, allow_single_quote_strings, true, false)
}

fn strip_php(src: &str) -> String {
    let mut out = strip_slash_comments_and_strings(src, true, true, false);
    let src_chars = chars(src);
    let mut out_chars = chars(&out);
    let mut i = 0;
    while i < src_chars.len() {
        if src_chars[i] == '#' {
            let start = i;
            while i < src_chars.len() && src_chars[i] != '\n' {
                i += 1;
            }
            blank_range(&mut out_chars, start, i, &src_chars);
        } else {
            i += 1;
        }
    }
    out = collect(out_chars);
    out
}

fn strip_go(src: &str) -> String {
    strip_slash_comments_and_strings(src, true, true, true)
}

fn strip_slash_comments_and_strings(
    src: &str,
    allow_single_quote_strings: bool,
    allow_backtick: bool,
    raw_backtick: bool,
) -> String {
    // C 风格语言共享这套扫描器：注释被清空，字符串只跳过不清空，避免字符串里的
    // `//` 或 `/*` 触发误判。
    let src = chars(src);
    let mut out = src.clone();
    let mut i = 0;
    while i < src.len() {
        let c = src[i];
        let c2 = src.get(i + 1).copied().unwrap_or('\0');
        if c == '/' && c2 == '*' {
            let start = i;
            i += 2;
            while i < src.len() && !(src[i] == '*' && src.get(i + 1) == Some(&'/')) {
                i += 1;
            }
            if i < src.len() {
                i += 2;
            }
            blank_range(&mut out, start, i, &src);
            continue;
        }
        if c == '/' && c2 == '/' {
            let start = i;
            while i < src.len() && src[i] != '\n' {
                i += 1;
            }
            blank_range(&mut out, start, i, &src);
            continue;
        }
        if c == '"' || (allow_single_quote_strings && c == '\'') || (allow_backtick && c == '`') {
            let quote = c;
            i += 1;
            while i < src.len() && src[i] != quote {
                if (!raw_backtick || quote != '`') && src[i] == '\\' && i + 1 < src.len() {
                    i += 2;
                    continue;
                }
                if quote != '`' && src[i] == '\n' {
                    break;
                }
                i += 1;
            }
            if i < src.len() && src[i] == quote {
                i += 1;
            }
            continue;
        }
        i += 1;
    }
    collect(out)
}

fn strip_rust(src: &str) -> String {
    // Rust 块注释允许嵌套，因此不能复用普通 C 风格的 `/* ... */` 扫描逻辑。
    let src = chars(src);
    let mut out = src.clone();
    let mut i = 0;
    while i < src.len() {
        let c = src[i];
        let c2 = src.get(i + 1).copied().unwrap_or('\0');
        if c == '/' && c2 == '*' {
            let start = i;
            i += 2;
            let mut depth = 1;
            while i < src.len() && depth > 0 {
                if src[i] == '/' && src.get(i + 1) == Some(&'*') {
                    depth += 1;
                    i += 2;
                } else if src[i] == '*' && src.get(i + 1) == Some(&'/') {
                    depth -= 1;
                    i += 2;
                } else {
                    i += 1;
                }
            }
            blank_range(&mut out, start, i, &src);
            continue;
        }
        if c == '/' && c2 == '/' {
            let start = i;
            while i < src.len() && src[i] != '\n' {
                i += 1;
            }
            blank_range(&mut out, start, i, &src);
            continue;
        }
        if c == '"' || c == '\'' {
            let quote = c;
            i += 1;
            while i < src.len() && src[i] != quote {
                if src[i] == '\\' && i + 1 < src.len() {
                    i += 2;
                } else if quote == '\'' && src[i] == '\n' {
                    break;
                } else {
                    i += 1;
                }
            }
            if i < src.len() && src[i] == quote {
                i += 1;
            }
            continue;
        }
        i += 1;
    }
    collect(out)
}

fn starts_with(src: &[char], idx: usize, needle: &str) -> bool {
    needle
        .chars()
        .enumerate()
        .all(|(offset, ch)| src.get(idx + offset) == Some(&ch))
}
