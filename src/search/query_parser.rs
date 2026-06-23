//! Field-qualified search query parser.
//!
//! Splits a raw query like
//!
//! ```text
//! kind:function name:auth path:src/api authenticate
//! ```
//!
//! into structured filters plus the free-text portion that goes to FTS.
//!
//! 中文维护提示：未知字段必须保留成普通文本，不能变成错误；MCP/CLI 搜索输入常
//! 混有 `TODO:` 或代码片段，宽容解析比严格失败更有用。

use crate::types::{Language, NodeKind};

/// Free-text query plus structured field filters.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ParsedQuery {
    /// Free-text portion to feed to FTS / LIKE. May be empty.
    pub text: String,
    /// `kind:` filters, OR'd by the caller.
    pub kinds: Vec<NodeKind>,
    /// `lang:` / `language:` filters, OR'd by the caller.
    pub languages: Vec<Language>,
    /// `path:` filters, case-insensitive substring of file_path.
    pub path_filters: Vec<String>,
    /// `name:` filters, case-insensitive substring of node.name.
    pub name_filters: Vec<String>,
}

fn unquote(value: &str) -> String {
    if value.len() >= 2 && value.starts_with('"') && value.ends_with('"') {
        value[1..value.len() - 1].to_string()
    } else {
        value.to_string()
    }
}

fn tokenize(raw: &str) -> Vec<String> {
    // tokenization 按字节边界切片，但遍历使用 char_indices，保证 UTF-8 查询文本
    // 不会被切在字符中间。
    let chars: Vec<(usize, char)> = raw.char_indices().collect();
    let mut tokens = Vec::new();
    let mut pos = 0;

    while pos < chars.len() {
        while pos < chars.len() && chars[pos].1.is_whitespace() {
            pos += 1;
        }
        if pos >= chars.len() {
            break;
        }

        let start_byte = chars[pos].0;
        let mut end_byte = raw.len();

        while pos < chars.len() && !chars[pos].1.is_whitespace() {
            if chars[pos].1 == '"' {
                // 引号只用于保留 token 内空格，不处理转义；未闭合引号会把余下内容
                // 当作同一个 token，避免用户输入半截查询时报错。
                pos += 1;
                while pos < chars.len() && chars[pos].1 != '"' {
                    pos += 1;
                }
                if pos >= chars.len() {
                    end_byte = raw.len();
                    break;
                }
                pos += 1;
                if pos >= chars.len() {
                    end_byte = raw.len();
                    break;
                }
                continue;
            }
            pos += 1;
            end_byte = if pos < chars.len() {
                chars[pos].0
            } else {
                raw.len()
            };
        }

        if pos < chars.len() {
            end_byte = chars[pos].0;
        }
        tokens.push(raw[start_byte..end_byte].to_string());
    }

    tokens
}

/// Parse a raw query into structured filters plus remaining text.
///
/// Unknown field prefixes are preserved as plain text so searches such as
/// `TODO:` still work instead of becoming parse errors.
pub fn parse_query(raw: &str) -> ParsedQuery {
    let mut out = ParsedQuery::default();
    let mut text_parts = Vec::new();

    for tok in tokenize(raw) {
        let Some(colon) = tok.find(':') else {
            text_parts.push(tok);
            continue;
        };
        if colon == 0 || colon == tok.len() - 1 {
            text_parts.push(tok);
            continue;
        }

        let key = tok[..colon].to_ascii_lowercase();
        let value_raw = unquote(&tok[colon + 1..]);
        if value_raw.is_empty() {
            text_parts.push(tok);
            continue;
        }

        match key.as_str() {
            "kind" => match parse_node_kind(&value_raw) {
                Ok(kind) => out.kinds.push(kind),
                Err(_) => text_parts.push(tok),
            },
            "lang" | "language" => {
                let lower = value_raw.to_ascii_lowercase();
                match parse_language(&lower) {
                    Ok(language) => out.languages.push(language),
                    Err(_) => text_parts.push(tok),
                }
            }
            "path" => out.path_filters.push(value_raw),
            "name" => out.name_filters.push(value_raw),
            _ => text_parts.push(tok),
        }
    }

    out.text = text_parts.join(" ").trim().to_string();
    out
}

fn parse_node_kind(value: &str) -> Result<NodeKind, serde_json::Error> {
    serde_json::from_value(serde_json::Value::String(value.to_string()))
}

fn parse_language(value: &str) -> Result<Language, serde_json::Error> {
    serde_json::from_value(serde_json::Value::String(value.to_string()))
}

/// Bounded edit distance with an early exit once the distance exceeds `max_dist`.
///
/// This mirrors the TypeScript implementation: it is plain Levenshtein distance
/// with O(min(a, b)) memory. Callers pass already case-folded strings.
pub fn bounded_edit_distance(a: &str, b: &str, max_dist: usize) -> usize {
    if a == b {
        return 0;
    }

    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let al = a_chars.len();
    let bl = b_chars.len();

    if al.abs_diff(bl) > max_dist {
        // 长度差已经超过上限时无需做 DP；返回 max+1 表示“不在可接受范围内”。
        return max_dist + 1;
    }
    if al == 0 {
        return bl;
    }
    if bl == 0 {
        return al;
    }

    let mut prev: Vec<usize> = (0..=bl).collect();
    let mut cur = vec![0; bl + 1];

    for i in 1..=al {
        cur[0] = i;
        let mut row_min = cur[0];

        for j in 1..=bl {
            let cost = usize::from(a_chars[i - 1] != b_chars[j - 1]);
            let insertion = cur[j - 1] + 1;
            let deletion = prev[j] + 1;
            let substitution = prev[j - 1] + cost;
            cur[j] = insertion.min(deletion).min(substitution);
            row_min = row_min.min(cur[j]);
        }

        if row_min > max_dist {
            // 每一行的最小值都超过阈值后，后续行不可能重新降回可接受范围。
            return max_dist + 1;
        }
        std::mem::swap(&mut prev, &mut cur);
    }

    prev[bl]
}
