//! JavaScript/TypeScript re-export extraction.
//!
//! barrel 文件只记录 re-export 关系，不直接产生命中节点；真正符号查找在
//! `exports.rs` 中按需沿链解析。

use std::sync::LazyLock;

use regex::Regex;

use crate::resolution::types::ReExport;
use crate::types::Language;

fn strip_js_comments(content: &str) -> String {
    // 注释里经常出现 `export ... from` 示例；剥注释时保留字符串内容，
    // 避免破坏真实导出语句里的路径字面量。
    let mut out = String::new();
    let mut chars = content.chars().peekable();
    let mut string_quote: Option<char> = None;
    while let Some(ch) = chars.next() {
        if let Some(quote) = string_quote {
            out.push(ch);
            if ch == '\\' {
                if let Some(next) = chars.next() {
                    out.push(next);
                }
            } else if ch == quote {
                string_quote = None;
            }
            continue;
        }
        if matches!(ch, '"' | '\'' | '`') {
            string_quote = Some(ch);
            out.push(ch);
            continue;
        }
        if ch == '/' && chars.peek().copied() == Some('/') {
            for next in chars.by_ref() {
                if next == '\n' {
                    out.push('\n');
                    break;
                }
            }
            continue;
        }
        if ch == '/' && chars.peek().copied() == Some('*') {
            chars.next();
            let mut prev = '\0';
            for next in chars.by_ref() {
                if prev == '*' && next == '/' {
                    break;
                }
                prev = next;
            }
            continue;
        }
        out.push(ch);
    }
    out
}

pub fn extract_re_exports(content: &str, language: Language) -> Vec<ReExport> {
    // 只覆盖 JS-family 语法；其他语言的 re-export/import all 由各自 resolver
    // 处理，避免混用规则。
    if !matches!(
        language,
        Language::TypeScript | Language::JavaScript | Language::Tsx | Language::Jsx
    ) {
        return Vec::new();
    }
    static WILDCARD_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"export\s*\*(?:\s+as\s+\w+)?\s*from\s*['"]([^'"]+)['"]"#).unwrap()
    });
    static NAMED_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"export\s*\{([^}]+)\}\s*from\s*['"]([^'"]+)['"]"#).unwrap());
    let cleaned = strip_js_comments(content);
    let mut out = Vec::new();
    for cap in WILDCARD_RE.captures_iter(&cleaned) {
        if let Some(source) = cap.get(1) {
            out.push(ReExport::Wildcard {
                source: source.as_str().to_string(),
            });
        }
    }
    for cap in NAMED_RE.captures_iter(&cleaned) {
        let inner = cap.get(1).map(|m| m.as_str()).unwrap_or("");
        let source = cap.get(2).map(|m| m.as_str()).unwrap_or("").to_string();
        for item in inner
            .split(',')
            .map(str::trim)
            .filter(|item| !item.is_empty())
        {
            if let Some((orig, alias)) = item.split_once(" as ") {
                out.push(ReExport::Named {
                    exported_name: alias.trim().to_string(),
                    original_name: orig.trim().to_string(),
                    source: source.clone(),
                });
            } else if item
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
            {
                out.push(ReExport::Named {
                    exported_name: item.to_string(),
                    original_name: item.to_string(),
                    source: source.clone(),
                });
            }
        }
    }
    out
}
