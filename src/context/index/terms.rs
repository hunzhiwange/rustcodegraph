//! Token and ranking helpers for context retrieval.
//!
//! 本模块把自然语言 query 中“像代码符号的片段”和普通检索词分开处理。
//! 这些规则故意偏保守：多召回一些候选可以靠后续排序修正，但把常见英文词当成符号
//! 会让 context 首屏偏离用户真正关心的结构。

use std::collections::{HashMap, HashSet};

use crate::types::SearchResult;

use super::graph_utils::node_sort_key;

pub(super) fn extract_symbols_from_query(query: &str) -> Vec<String> {
    let mut symbols = Vec::<String>::new();
    let mut seen = HashSet::<String>::new();

    for token in identifier_tokens(query) {
        if token.contains('.') && is_dot_notation(&token) {
            // `Class.method` 同时保留完整限定名和各段，兼顾精确命中与未建限定名的索引。
            add_symbol(&mut symbols, &mut seen, token.clone());
            for part in token.split('.') {
                if part.len() >= 2 {
                    add_symbol(&mut symbols, &mut seen, part.to_string());
                }
            }
            continue;
        }

        if is_camel_identifier(&token)
            || is_snake_identifier(&token)
            || is_screaming_snake(&token)
            || is_acronym(&token)
            || is_lower_identifier(&token)
        {
            add_symbol(&mut symbols, &mut seen, token);
        }
    }

    symbols
        .into_iter()
        .filter(|symbol| !is_common_symbol_word(&symbol.to_ascii_lowercase()))
        .collect()
}

fn identifier_tokens(query: &str) -> Vec<String> {
    // 这里只识别 ASCII 代码标识符形状；自然语言的 Unicode 文本走全文搜索即可。
    let mut tokens = Vec::new();
    let mut current = String::new();
    for ch in query.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '.' {
            current.push(ch);
        } else if !current.is_empty() {
            push_trimmed_token(&mut tokens, &mut current);
        }
    }
    if !current.is_empty() {
        push_trimmed_token(&mut tokens, &mut current);
    }
    tokens
}

fn push_trimmed_token(tokens: &mut Vec<String>, current: &mut String) {
    let token = current.trim_matches('.').to_string();
    if !token.is_empty() {
        tokens.push(token);
    }
    current.clear();
}

fn add_symbol(symbols: &mut Vec<String>, seen: &mut HashSet<String>, symbol: String) {
    if symbol.len() >= 2 && seen.insert(symbol.clone()) {
        symbols.push(symbol);
    }
}

fn is_dot_notation(token: &str) -> bool {
    let mut parts = token.split('.');
    let Some(first) = parts.next() else {
        return false;
    };
    is_identifier_part(first) && parts.all(is_identifier_part)
}

fn is_identifier_part(part: &str) -> bool {
    let mut chars = part.chars();
    chars.next().is_some_and(|ch| ch.is_ascii_alphabetic())
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

fn is_camel_identifier(token: &str) -> bool {
    if token.len() < 2
        || !token
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_alphabetic())
    {
        return false;
    }
    let chars = token.chars().collect::<Vec<_>>();
    (1..chars.len()).any(|idx| {
        chars[idx].is_ascii_uppercase()
            && (chars[idx - 1].is_ascii_lowercase()
                || chars[idx - 1].is_ascii_digit()
                || chars
                    .get(idx + 1)
                    .is_some_and(|next| next.is_ascii_lowercase()))
    })
}

fn is_snake_identifier(token: &str) -> bool {
    token.len() >= 3
        && token.contains('_')
        && token
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_alphabetic())
        && token
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

fn is_screaming_snake(token: &str) -> bool {
    token.contains('_')
        && token
            .chars()
            .all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit() || ch == '_')
}

fn is_acronym(token: &str) -> bool {
    token.len() >= 2 && token.chars().all(|ch| ch.is_ascii_uppercase())
}

fn is_lower_identifier(token: &str) -> bool {
    token.len() >= 3
        && token
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_lowercase())
        && token
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit())
}

fn is_common_symbol_word(word: &str) -> bool {
    // lower_identifier 允许 `render` 这类真实函数名，但也会捞到大量 prose 常词。
    // 这份停用词表是 context symbol 提取的安全阀，新增词时要确认不会屏蔽常见 API 名。
    matches!(
        word,
        "the"
            | "and"
            | "for"
            | "with"
            | "from"
            | "this"
            | "that"
            | "have"
            | "been"
            | "will"
            | "would"
            | "could"
            | "should"
            | "does"
            | "done"
            | "make"
            | "made"
            | "use"
            | "used"
            | "using"
            | "work"
            | "works"
            | "find"
            | "found"
            | "show"
            | "call"
            | "called"
            | "calling"
            | "get"
            | "set"
            | "add"
            | "all"
            | "any"
            | "how"
            | "what"
            | "when"
            | "where"
            | "which"
            | "who"
            | "why"
            | "not"
            | "but"
            | "are"
            | "was"
            | "were"
            | "has"
            | "had"
            | "its"
            | "can"
            | "did"
            | "may"
            | "also"
            | "into"
            | "than"
            | "then"
            | "them"
            | "each"
            | "other"
            | "some"
            | "such"
            | "only"
            | "same"
            | "about"
            | "after"
            | "before"
            | "between"
            | "through"
            | "during"
            | "without"
            | "again"
            | "further"
            | "once"
            | "here"
            | "there"
            | "both"
            | "just"
            | "more"
            | "most"
            | "very"
            | "being"
            | "having"
            | "doing"
            | "system"
            | "need"
            | "needs"
            | "want"
            | "wants"
            | "like"
            | "look"
            | "change"
            | "changes"
            | "changed"
            | "changing"
            | "layer"
            | "handle"
            | "handles"
            | "handling"
            | "incoming"
            | "outgoing"
            | "data"
            | "flow"
            | "flows"
            | "level"
            | "levels"
            | "request"
            | "requests"
            | "response"
            | "responses"
            | "implement"
            | "implements"
            | "implementation"
            | "interface"
            | "interfaces"
            | "class"
            | "classes"
            | "method"
            | "methods"
            | "trigger"
            | "triggers"
            | "affected"
            | "affect"
            | "affects"
            | "else"
            | "code"
            | "failing"
            | "failed"
            | "silently"
            | "decide"
            | "decides"
            | "return"
            | "returns"
            | "returned"
            | "take"
            | "takes"
            | "taken"
            | "check"
            | "checks"
            | "checked"
            | "create"
            | "creates"
            | "created"
            | "read"
            | "reads"
            | "write"
            | "writes"
            | "written"
            | "start"
            | "starts"
            | "stop"
            | "stops"
            | "run"
            | "runs"
            | "running"
    )
}

pub(super) fn title_case_identifier(symbol: &str) -> String {
    let mut chars = symbol.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    format!(
        "{}{}",
        first.to_ascii_uppercase(),
        chars.as_str().to_ascii_lowercase()
    )
}

pub(super) fn truncate_to_char_boundary(value: &str, max_bytes: usize) -> &str {
    if value.len() <= max_bytes {
        return value;
    }
    let mut end = max_bytes;
    // max_code_block_size 按字节配置；截断 UTF-8 时必须回退到 char 边界。
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    &value[..end]
}

pub(super) fn sort_results(results: &mut [SearchResult]) {
    // 分数相同使用稳定的源码位置排序，避免同一 query 在不同 HashMap 顺序下输出抖动。
    results.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then_with(|| node_sort_key(&a.node).cmp(&node_sort_key(&b.node)))
    });
}

pub(super) fn upsert_result(
    results: &mut Vec<SearchResult>,
    index: &mut HashMap<String, usize>,
    result: SearchResult,
) {
    if let Some(existing_index) = index.get(&result.node.id).copied() {
        results[existing_index].score = results[existing_index].score.max(result.score);
    } else {
        index.insert(result.node.id.clone(), results.len());
        results.push(result);
    }
}

pub(super) fn grouped_substring_terms(mut terms: Vec<String>) -> Vec<Vec<String>> {
    terms.sort_by(|a, b| b.len().cmp(&a.len()).then(a.cmp(b)));
    let mut assigned = HashSet::<String>::new();
    let mut groups = Vec::new();

    // capture/capturing 这类互含词只算一个概念，避免重复词形把候选分数放大。
    for term in &terms {
        if assigned.contains(term) {
            continue;
        }
        let mut group = vec![term.clone()];
        assigned.insert(term.clone());
        for other in &terms {
            if assigned.contains(other) {
                continue;
            }
            if term.contains(other) || other.contains(term) {
                group.push(other.clone());
                assigned.insert(other.clone());
            }
        }
        groups.push(group);
    }

    groups
}

pub(super) fn dominant_file_dir(file_path: &str) -> Option<String> {
    file_path
        .rfind('/')
        .filter(|idx| *idx > 0)
        .map(|idx| file_path[..=idx].to_string())
}

pub(super) fn path_dirname(file_path: &str) -> String {
    // DB 中路径多为 `/`，但测试和 Windows 输入可能带 `\`；这里归一后再取目录。
    let normalized = file_path.replace('\\', "/");
    normalized
        .rfind('/')
        .map(|idx| {
            if idx == 0 {
                "/".to_string()
            } else {
                normalized[..idx].to_string()
            }
        })
        .unwrap_or_else(|| ".".to_string())
}
