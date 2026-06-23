//! Search query utilities: term extraction and ranking helpers.
//!
//! 中文维护提示：这些函数把自然语言问题拆成可用于 FTS/排序的代码词元。目标是
//! 提升命中质量，而不是做完整 NLP；启发式要保持可预测、低成本。

use std::collections::HashSet;
use std::fs;
use std::path::Path;

use crate::types::{Node, NodeKind};

pub const STOP_WORDS: &[&str] = &[
    "the",
    "a",
    "an",
    "and",
    "or",
    "but",
    "in",
    "on",
    "at",
    "to",
    "for",
    "of",
    "with",
    "by",
    "from",
    "is",
    "it",
    "that",
    "this",
    "are",
    "was",
    "be",
    "has",
    "had",
    "have",
    "do",
    "does",
    "did",
    "will",
    "would",
    "could",
    "should",
    "may",
    "might",
    "can",
    "shall",
    "not",
    "no",
    "all",
    "each",
    "every",
    "how",
    "what",
    "where",
    "when",
    "who",
    "which",
    "why",
    "i",
    "me",
    "my",
    "we",
    "our",
    "you",
    "your",
    "he",
    "she",
    "they",
    "show",
    "give",
    "tell",
    "been",
    "done",
    "made",
    "used",
    "using",
    "work",
    "works",
    "found",
    "also",
    "into",
    "then",
    "than",
    "just",
    "more",
    "some",
    "such",
    "over",
    "only",
    "out",
    "its",
    "so",
    "up",
    "as",
    "if",
    "look",
    "need",
    "needs",
    "want",
    "happen",
    "happens",
    "affect",
    "affected",
    "break",
    "breaks",
    "failing",
    "implemented",
    "implement",
    "code",
    "file",
    "files",
    "function",
    "method",
    "class",
    "type",
    "fix",
    "bug",
    "called",
];

fn is_stop_word(term: &str) -> bool {
    STOP_WORDS.contains(&term)
}

fn add_unique(out: &mut Vec<String>, seen: &mut HashSet<String>, value: String) {
    if seen.insert(value.clone()) {
        out.push(value);
    }
}

/// Normalize a name to a comparable token: lowercase, alphanumerics only.
pub fn normalize_name_token(raw: &str) -> String {
    raw.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

/// Tokens that name the project as a whole rather than a specific symbol.
pub fn derive_project_name_tokens(project_root: &str) -> HashSet<String> {
    // 项目名在用户问题里通常只是上下文噪声；从 go.mod、package.json 和目录名
    // 提取后，可在路径排序时降低它的影响。
    let mut tokens = HashSet::new();
    let mut add = |raw: Option<&str>| {
        let Some(raw) = raw else {
            return;
        };
        let norm = normalize_name_token(raw);
        if norm.len() >= 5 {
            tokens.insert(norm);
        }
    };

    if let Ok(gomod) = fs::read_to_string(Path::new(project_root).join("go.mod")) {
        for line in gomod.lines() {
            let trimmed = line.trim_start();
            if let Some(rest) = trimmed.strip_prefix("module ") {
                add(rest
                    .split_whitespace()
                    .next()
                    .and_then(|m| m.rsplit('/').next()));
                break;
            }
        }
    }

    if let Ok(pkg) = fs::read_to_string(Path::new(project_root).join("package.json"))
        && let Ok(value) = serde_json::from_str::<serde_json::Value>(&pkg)
        && let Some(name) = value.get("name").and_then(|v| v.as_str())
    {
        add(Some(
            name.rsplit_once('/').map(|(_, name)| name).unwrap_or(name),
        ));
    }

    add(Path::new(project_root)
        .canonicalize()
        .ok()
        .as_deref()
        .and_then(Path::file_name)
        .and_then(|s| s.to_str())
        .or_else(|| Path::new(project_root).file_name().and_then(|s| s.to_str())));

    tokens
}

/// Generate stem variants of a search term by removing common English suffixes.
pub fn get_stem_variants(term: &str) -> Vec<String> {
    // 简单英文词干变体足够覆盖 "rendering"→"render" 等搜索场景；不引入词干库，
    // 避免依赖和跨语言行为变复杂。
    let mut variants = Vec::new();
    let mut seen = HashSet::new();
    let t = term.to_ascii_lowercase();

    if t.ends_with("ing") && t.len() > 5 {
        let base = &t[..t.len() - 3];
        add_unique(&mut variants, &mut seen, base.to_string());
        add_unique(&mut variants, &mut seen, format!("{base}e"));
        if has_doubled_last_char(base) {
            add_unique(&mut variants, &mut seen, base[..base.len() - 1].to_string());
        }
    }

    if (t.ends_with("tion") || t.ends_with("sion")) && t.len() > 5 {
        add_unique(&mut variants, &mut seen, t[..t.len() - 3].to_string());
    }

    if t.ends_with("ment") && t.len() > 6 {
        add_unique(&mut variants, &mut seen, t[..t.len() - 4].to_string());
    }

    if t.ends_with("ies") && t.len() > 4 {
        add_unique(&mut variants, &mut seen, format!("{}y", &t[..t.len() - 3]));
    } else if t.ends_with("es") && t.len() > 4 {
        add_unique(&mut variants, &mut seen, t[..t.len() - 2].to_string());
    } else if t.ends_with('s') && !t.ends_with("ss") && t.len() > 4 {
        add_unique(&mut variants, &mut seen, t[..t.len() - 1].to_string());
    }

    if t.ends_with("ed") && !t.ends_with("eed") && t.len() > 4 {
        add_unique(&mut variants, &mut seen, t[..t.len() - 1].to_string());
        add_unique(&mut variants, &mut seen, t[..t.len() - 2].to_string());
        if t.ends_with("ied") && t.len() > 5 {
            add_unique(&mut variants, &mut seen, format!("{}y", &t[..t.len() - 3]));
        }
    }

    if t.ends_with("er") && t.len() > 4 {
        let base = &t[..t.len() - 2];
        add_unique(&mut variants, &mut seen, base.to_string());
        add_unique(&mut variants, &mut seen, format!("{base}e"));
        if has_doubled_last_char(base) {
            add_unique(&mut variants, &mut seen, base[..base.len() - 1].to_string());
        }
    }

    variants
        .into_iter()
        .filter(|variant| variant.len() >= 3 && variant != &t)
        .collect()
}

fn has_doubled_last_char(value: &str) -> bool {
    let mut chars = value.chars().rev();
    matches!((chars.next(), chars.next()), (Some(a), Some(b)) if a == b)
}

/// Extract meaningful search terms from a natural-language query.
pub fn extract_search_terms(query: &str, include_stems: Option<bool>) -> Vec<String> {
    let include_stems = include_stems.unwrap_or(true);
    let mut out = Vec::new();
    let mut seen = HashSet::new();

    // 先保留完整的 CamelCase/PascalCase 标识符，再拆成子词；这样精确符号名和
    // 自然语言描述都能参与检索。
    for token in ascii_word_runs(query, false) {
        if token.len() >= 3 && is_compound_identifier(&token) {
            add_unique(&mut out, &mut seen, token.to_ascii_lowercase());
        }
    }

    for token in ascii_word_runs(query, true) {
        if token.len() >= 3
            && token.contains('_')
            && token
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic())
        {
            add_unique(&mut out, &mut seen, token.to_ascii_lowercase());
        }
    }

    for word in split_query_words(query) {
        let lower = word.to_ascii_lowercase();
        if lower.len() < 3 || is_stop_word(&lower) {
            continue;
        }
        add_unique(&mut out, &mut seen, lower);
    }

    if include_stems {
        let base_terms = out.clone();
        for token in base_terms {
            for variant in get_stem_variants(&token) {
                if !is_stop_word(&variant) {
                    add_unique(&mut out, &mut seen, variant);
                }
            }
        }
    }

    out
}

fn ascii_word_runs(query: &str, include_underscore: bool) -> Vec<String> {
    let mut runs = Vec::new();
    let mut cur = String::new();
    for ch in query.chars() {
        let keep = ch.is_ascii_alphanumeric() || (include_underscore && ch == '_');
        if keep {
            cur.push(ch);
        } else if !cur.is_empty() {
            runs.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        runs.push(cur);
    }
    runs
}

fn is_compound_identifier(token: &str) -> bool {
    let chars: Vec<char> = token.chars().collect();
    if chars.len() < 3 || !chars[0].is_ascii_alphabetic() {
        return false;
    }
    (1..chars.len()).any(|i| {
        chars[i].is_ascii_uppercase()
            && (chars[i - 1].is_ascii_lowercase()
                || chars[i - 1].is_ascii_digit()
                || chars
                    .get(i + 1)
                    .is_some_and(|next| next.is_ascii_lowercase()))
    })
}

fn split_query_words(query: &str) -> Vec<String> {
    let mut words = Vec::new();
    for run in ascii_word_runs(query, true) {
        for part in run.split('_').filter(|part| !part.is_empty()) {
            split_camel_like(part, &mut words);
        }
    }
    words
}

fn split_camel_like(token: &str, out: &mut Vec<String>) {
    let chars: Vec<char> = token.chars().collect();
    if chars.is_empty() {
        return;
    }

    let mut start = 0;
    for i in 1..chars.len() {
        let prev = chars[i - 1];
        let cur = chars[i];
        let next = chars.get(i + 1).copied();
        let boundary = (prev.is_ascii_lowercase() && cur.is_ascii_uppercase())
            || (prev.is_ascii_uppercase()
                && cur.is_ascii_uppercase()
                && next.is_some_and(|n| n.is_ascii_lowercase()));
        if boundary {
            out.push(chars[start..i].iter().collect());
            start = i;
        }
    }
    out.push(chars[start..].iter().collect());
}

/// Score path relevance to a query.
pub fn score_path_relevance(
    file_path: &str,
    query: &str,
    project_name_tokens: Option<&HashSet<String>>,
) -> f64 {
    // 路径得分只作为搜索排序的一部分：文件名权重大于目录名，目录名大于完整路径
    // 子串，非测试查询会轻微压低测试/fixture 文件。
    let normalized_path = file_path.replace('\\', "/");
    let path_lower = normalized_path.to_ascii_lowercase();
    let file_name = path_basename(&normalized_path).to_ascii_lowercase();
    let dir_name = path_dirname(&normalized_path).to_ascii_lowercase();
    let mut score = 0;

    let all_words: Vec<&str> = query.split_whitespace().filter(|w| !w.is_empty()).collect();
    if all_words.is_empty() {
        return 0.0;
    }

    let words: Vec<&str> = if let Some(project_name_tokens) = project_name_tokens {
        if project_name_tokens.is_empty() {
            all_words.clone()
        } else {
            all_words
                .iter()
                .copied()
                .filter(|w| !project_name_tokens.contains(&normalize_name_token(w)))
                .collect()
        }
    } else {
        all_words.clone()
    };
    let scored = if words.is_empty() { all_words } else { words };

    for word in scored {
        let subtokens = extract_search_terms(word, Some(false));
        if subtokens.is_empty() {
            continue;
        }
        if subtokens.iter().any(|t| file_name.contains(t)) {
            score += 10;
        }
        if subtokens.iter().any(|t| dir_name.contains(t)) {
            score += 5;
        } else if subtokens.iter().any(|t| path_lower.contains(t)) {
            score += 3;
        }
    }

    let query_lower = query.to_ascii_lowercase();
    let is_test_query = query_lower.contains("test") || query_lower.contains("spec");
    if !is_test_query && is_test_file(file_path) {
        score -= 15;
    }

    score as f64
}

fn path_basename(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

fn path_dirname(path: &str) -> &str {
    path.rfind(['/', '\\'])
        .map(|idx| if idx == 0 { &path[..1] } else { &path[..idx] })
        .unwrap_or(".")
}

/// Check if a file path looks like a test file.
pub fn is_test_file(file_path: &str) -> bool {
    // 同时识别目录约定、分隔符后缀和 CamelCase 后缀，覆盖 JS/Python/Java/C# 等常见
    // 测试命名方式。
    let normalized = file_path.replace('\\', "/");
    let lower = normalized.to_ascii_lowercase();
    let file_name = path_basename(&normalized);
    let lower_name = file_name.to_ascii_lowercase();

    if lower_name.starts_with("test_") || lower_name.starts_with("test.") {
        return true;
    }

    if has_separator_delimited_test_suffix(&lower_name) {
        return true;
    }

    if has_camel_test_suffix(file_name) {
        return true;
    }

    if lower.contains("/tests/")
        || lower.contains("/test/")
        || lower.contains("/__tests__/")
        || lower.contains("/spec/")
        || lower.contains("/specs/")
        || lower.contains("/testlib/")
        || lower.contains("/testing/")
        || lower.starts_with("test/")
        || lower.starts_with("tests/")
        || lower.starts_with("spec/")
        || lower.starts_with("specs/")
        || has_camel_test_dir(&normalized)
    {
        return true;
    }

    matches_non_production_dir(&lower)
}

fn has_separator_delimited_test_suffix(lower_name: &str) -> bool {
    let Some(dot) = lower_name.rfind('.') else {
        return false;
    };
    let stem = &lower_name[..dot];
    let ext = &lower_name[dot + 1..];
    if ext.is_empty() || !ext.chars().all(|c| c.is_ascii_alphanumeric()) {
        return false;
    }
    stem.rsplit(['.', '_', '-'])
        .next()
        .is_some_and(|segment| matches!(segment, "test" | "tests" | "spec" | "specs"))
}

fn has_camel_test_suffix(file_name: &str) -> bool {
    let Some(dot) = file_name.rfind('.') else {
        return false;
    };
    let stem = &file_name[..dot];
    let ext = &file_name[dot + 1..];
    !ext.is_empty()
        && ext.chars().all(|c| c.is_ascii_alphanumeric())
        && ["Test", "Tests", "TestCase", "Tester", "Spec", "Specs"]
            .iter()
            .any(|suffix| stem.ends_with(suffix))
}

fn has_camel_test_dir(file_path: &str) -> bool {
    let mut components: Vec<&str> = file_path.split('/').collect();
    components.pop();
    components.iter().any(|component| {
        !component.is_empty()
            && ["Test", "Tests", "Spec"]
                .iter()
                .any(|suffix| component.ends_with(suffix))
    })
}

fn matches_non_production_dir(lower_path: &str) -> bool {
    const DIRS: &[&str] = &[
        "integration",
        "sample",
        "samples",
        "example",
        "examples",
        "fixture",
        "fixtures",
        "benchmark",
        "benchmarks",
        "demo",
        "demos",
    ];
    DIRS.iter().any(|dir| {
        lower_path.contains(&format!("/{dir}/")) || lower_path.starts_with(&format!("{dir}/"))
    })
}

/// Bonus when a node's name matches the search query.
pub fn name_match_bonus(node_name: &str, query: &str) -> f64 {
    // 精确名匹配的奖励明显高于包含匹配，确保 `authenticate` 这类明确查询不会被
    // 较长但包含同词的符号压过。
    let name_lower = node_name.to_ascii_lowercase();
    let raw_terms: Vec<String> = split_for_name_bonus(query)
        .into_iter()
        .map(|term| term.to_ascii_lowercase())
        .filter(|term| term.len() >= 2)
        .collect();
    let query_tokens: Vec<String> = query
        .split_whitespace()
        .map(|term| term.to_ascii_lowercase())
        .filter(|term| term.len() >= 2)
        .collect();
    let query_lower = query
        .split_whitespace()
        .collect::<String>()
        .to_ascii_lowercase();

    if name_lower == query_lower {
        return 80.0;
    }
    if query_tokens.len() > 1 && query_tokens.iter().any(|term| term == &name_lower) {
        return 60.0;
    }
    if !query_lower.is_empty() && name_lower.starts_with(&query_lower) {
        let ratio = query_lower.len() as f64 / name_lower.len() as f64;
        return (10.0 + 30.0 * ratio).round();
    }
    if raw_terms.len() > 1 && raw_terms.iter().all(|term| name_lower.contains(term)) {
        return 15.0;
    }
    if !query_lower.is_empty() && name_lower.contains(&query_lower) {
        return 10.0;
    }
    0.0
}

fn split_for_name_bonus(query: &str) -> Vec<String> {
    let mut words = Vec::new();
    for part in query.split(|c: char| c.is_whitespace() || c == '_' || c == '.' || c == '-') {
        if !part.is_empty() {
            split_camel_like(part, &mut words);
        }
    }
    words
}

/// Kind-based bonus for search ranking.
pub fn kind_bonus(kind: impl std::borrow::Borrow<NodeKind>) -> f64 {
    match *kind.borrow() {
        NodeKind::Function | NodeKind::Method => 10.0,
        NodeKind::Interface | NodeKind::Trait | NodeKind::Route | NodeKind::Protocol => 9.0,
        NodeKind::Class | NodeKind::Component => 8.0,
        NodeKind::TypeAlias | NodeKind::Struct => 6.0,
        NodeKind::Enum => 5.0,
        NodeKind::Module | NodeKind::Namespace => 4.0,
        NodeKind::Property | NodeKind::Field | NodeKind::Constant | NodeKind::EnumMember => 3.0,
        NodeKind::Variable => 2.0,
        NodeKind::Import | NodeKind::Export => 1.0,
        NodeKind::Parameter | NodeKind::File => 0.0,
    }
}

/// Whether a query token looks like a deliberately typed code identifier.
pub fn is_distinctive_identifier(token: &str) -> bool {
    // 下划线、数字或内部大写通常表示用户输入了真实代码标识符，而非普通英文词。
    if token.is_empty() {
        return false;
    }
    if token.chars().any(|c| c == '_' || c.is_ascii_digit()) {
        return true;
    }
    token.chars().skip(1).any(|c| c.is_ascii_uppercase())
}

/// Convenience wrapper for callers that already have a node.
pub fn node_name_match_bonus(node: &Node, query: &str) -> f64 {
    name_match_bonus(&node.name, query)
}
