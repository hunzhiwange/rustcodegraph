//! Dynamic-dispatch boundary detection translated from
//! `dynamic-boundaries.ts`.
//!
//! 这里不尝试解析所有动态分派，只扫描少量高信号 runtime 边界，并把结果
//! 作为 MCP 输出中的“静态路径到此为止”提示，避免伪造不存在的图边。

use std::collections::{HashMap, HashSet};

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::resolution::strip_comments::{CommentLang, strip_comments_for_regex};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoundaryMatch {
    pub form: String,
    pub label: String,
    pub snippet: String,
    pub line: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_is_type: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub more_sites: Option<u64>,
}

const JS_FAMILY: &[&str] = &[
    "typescript",
    "javascript",
    "tsx",
    "jsx",
    "vue",
    "svelte",
    "astro",
];
const PY: &[&str] = &["python"];
const RB: &[&str] = &["ruby"];
const PHP: &[&str] = &["php"];
const JVM_CS_GO: &[&str] = &["java", "kotlin", "scala", "csharp", "go"];
const SWIFT_OBJC: &[&str] = &["swift", "objc", "objcpp", "objective-c"];

const MAX_MATCHES_PER_BODY: usize = 3;
const MAX_BODY_CHARS: usize = 60_000;
const MAX_GETATTR_ARGS: usize = 300;

#[derive(Debug, Clone, Copy)]
enum KeyMode {
    None,
    ComputedCall,
    SingleString,
    RubySend,
    TypedBus,
    Selector,
}

struct FormSpec<'a> {
    form: &'a str,
    label: &'a str,
    langs: Option<&'a [&'a str]>,
    pattern: &'a str,
    key_mode: KeyMode,
    key_window: Option<usize>,
}

const FORMS: &[FormSpec<'_>] = &[
    FormSpec {
        form: "computed-call",
        label: "computed member call",
        langs: None,
        pattern: r#"[\w$)\]]\s*\[([^\[\]\n]{1,80})\]\s*\("#,
        key_mode: KeyMode::ComputedCall,
        key_window: None,
    },
    FormSpec {
        form: "dynamic-import",
        label: "dynamic import",
        langs: Some(JS_FAMILY),
        pattern: r#"\b(?:import|require)\s*\("#,
        key_mode: KeyMode::None,
        key_window: None,
    },
    FormSpec {
        form: "dynamic-import",
        label: "dynamic import",
        langs: Some(PY),
        pattern: r#"\bimportlib\.import_module\s*\(|\b__import__\s*\("#,
        key_mode: KeyMode::None,
        key_window: None,
    },
    FormSpec {
        form: "ruby-send",
        label: "send dispatch",
        langs: Some(RB),
        pattern: r#"\.(?:public_)?send\s*\(\s*:?\w+|\bmethod\s*\(\s*:\w+\s*\)"#,
        key_mode: KeyMode::RubySend,
        key_window: None,
    },
    FormSpec {
        form: "php-dynamic",
        label: "dynamic call",
        langs: Some(PHP),
        pattern: r#"\bcall_user_func(?:_array)?\s*\(|\$this\s*->\s*\$\w+\s*\(|\$\w+\s*\("#,
        key_mode: KeyMode::SingleString,
        key_window: Some(80),
    },
    FormSpec {
        form: "reflection",
        label: "reflective dispatch",
        langs: Some(JVM_CS_GO),
        pattern: r#"\.invoke\s*\(|\.get(?:Declared)?Method\s*\(|\.GetMethod\s*\(|MethodByName\s*\(|Activator\.CreateInstance|Class\.forName\s*\("#,
        key_mode: KeyMode::SingleString,
        key_window: Some(80),
    },
    FormSpec {
        form: "proxy-reflect",
        label: "Proxy/Reflect dispatch",
        langs: Some(JS_FAMILY),
        pattern: r#"\bnew\s+Proxy\s*\(|\bReflect\.(?:get|apply|construct)\s*\("#,
        key_mode: KeyMode::None,
        key_window: None,
    },
    FormSpec {
        form: "typed-bus",
        label: "typed message dispatch",
        langs: None,
        pattern: r#"\.(?:[Ss]end|[Pp]ublish|[Dd]ispatch|[Ee]xecute|[Pp]ost|[Ee]mit)(?:Async)?\s*(?:<[^<>\n]{0,80}>)?\s*\(\s*new\s+([A-Z]\w*)"#,
        key_mode: KeyMode::TypedBus,
        key_window: None,
    },
    FormSpec {
        form: "var-key-dispatch",
        label: "string-keyed dispatch (runtime key)",
        langs: None,
        pattern: r#"\.(?:emit|dispatch|trigger|fire|publish|broadcast)\s*\(\s*[A-Za-z_$][\w$]*(?:\.[\w$]+){0,3}\s*[,)]"#,
        key_mode: KeyMode::None,
        key_window: None,
    },
    FormSpec {
        form: "selector",
        label: "selector dispatch",
        langs: Some(SWIFT_OBJC),
        pattern: r#"#selector\s*\(\s*([\w.]+)|NSClassFromString\s*\("#,
        key_mode: KeyMode::Selector,
        key_window: None,
    },
];

pub fn blank_string_contents(text: &str) -> String {
    // 保留字符串外的 byte/line 布局，但抹掉字符串内容；这样 regex 不会把
    // 注释或字符串里的示例代码误当真实 dispatch。
    let mut out: Vec<char> = text.chars().collect();
    let src = out.clone();
    let mut i = 0;
    while i < src.len() {
        let c = src[i];
        if c == '"' || c == '\'' || c == '`' {
            let quote = c;
            i += 1;
            while i < src.len() && src[i] != quote {
                if src[i] == '\\' && i + 1 < src.len() {
                    out[i] = ' ';
                    out[i + 1] = ' ';
                    i += 2;
                    continue;
                }
                if quote != '`' && src[i] == '\n' {
                    break;
                }
                if src[i] != '\n' {
                    out[i] = ' ';
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
    out.into_iter().collect()
}

pub fn scan_dynamic_dispatch(
    body: &str,
    language: &str,
    file_start_line: u64,
) -> Vec<BoundaryMatch> {
    // 限制扫描体积和每个 body 的输出数量：dynamic boundary 是诊断信号，
    // 不是搜索结果列表，过多会挤掉真正需要的源代码。
    let original: String = body.chars().take(MAX_BODY_CHARS).collect();
    let stripped = match comment_lang(language) {
        Some(lang) => blank_string_contents(&strip_comments_for_regex(&original, lang)),
        None => blank_string_contents(&original),
    };

    let mut out = Vec::new();
    let mut seen = HashMap::<String, usize>::new();

    if language == "python" {
        scan_python_getattr(&stripped, &original, file_start_line, &mut out, &mut seen);
    }

    for spec in FORMS {
        if out.len() >= MAX_MATCHES_PER_BODY {
            break;
        }
        if let Some(langs) = spec.langs
            && !langs.contains(&language)
        {
            continue;
        }
        let Ok(re) = Regex::new(spec.pattern) else {
            continue;
        };
        for mat in re.find_iter(&stripped) {
            if out.len() >= MAX_MATCHES_PER_BODY {
                return out;
            }
            let mut slice_end = mat.end();
            if let Some(window) = spec.key_window {
                let window_end = (slice_end + window).min(original.len());
                let nl = original[slice_end..]
                    .find('\n')
                    .map(|idx| slice_end + idx)
                    .unwrap_or(window_end);
                slice_end = nl.min(window_end);
            }
            let orig_slice = safe_slice(&original, mat.start(), slice_end);
            let (key, key_is_type) = derive_key(spec.key_mode, orig_slice);
            push_match(
                &mut out,
                &mut seen,
                spec.form,
                spec.label,
                &original,
                mat.start(),
                file_start_line,
                key,
                key_is_type,
            );
        }
    }
    out
}

fn scan_python_getattr(
    stripped: &str,
    original: &str,
    file_start_line: u64,
    out: &mut Vec<BoundaryMatch>,
    seen: &mut HashMap<String, usize>,
) {
    let Ok(re) = Regex::new(r#"\bgetattr\s*\("#) else {
        return;
    };
    let call_after_re = Regex::new(r#"^\s*\("#).unwrap();
    let assign_re = Regex::new(r#"(\w+)\s*=\s*$"#).unwrap();
    for mat in re.find_iter(stripped) {
        if out.len() >= MAX_MATCHES_PER_BODY {
            return;
        }
        let open = mat.end() - 1;
        let close = match_balanced_paren(stripped, open);
        if close < 0 {
            continue;
        }
        let close = close as usize;
        let after = safe_slice(stripped, close + 1, (close + 8).min(stripped.len()));
        let mut form = None;
        let mut label = "";
        if call_after_re.is_match(after) {
            form = Some("getattr-call");
            label = "getattr dispatch";
        } else {
            let line_start = stripped[..mat.start()]
                .rfind('\n')
                .map(|idx| idx + 1)
                .unwrap_or(0);
            let before = &stripped[line_start..mat.start()];
            if let Some(assign) = assign_re.captures(before) {
                let var_name = &assign[1];
                let call_re =
                    Regex::new(&format!(r#"\b{}\s*\("#, regex::escape(var_name))).unwrap();
                if call_re.is_match(&stripped[close + 1..]) {
                    form = Some("getattr-assign");
                    label = "getattr dispatch (assigned, called later)";
                }
            }
        }
        let Some(form) = form else {
            continue;
        };
        let key = single_string_literal(safe_slice(original, open + 1, close));
        push_match(
            out,
            seen,
            form,
            label,
            original,
            mat.start(),
            file_start_line,
            key,
            None,
        );
    }
}

fn derive_key(mode: KeyMode, text: &str) -> (Option<String>, Option<bool>) {
    match mode {
        KeyMode::None => (None, None),
        KeyMode::ComputedCall => {
            let Ok(re) = Regex::new(r#"\[([^\[\]\n]{1,80})\]\s*\($"#) else {
                return (None, None);
            };
            let key = re
                .captures(text)
                .and_then(|cap| cap.get(1))
                .and_then(|m| single_string_literal(m.as_str()));
            (key, None)
        }
        KeyMode::SingleString => (single_string_literal(text), None),
        KeyMode::RubySend => {
            let Ok(re) = Regex::new(r#":(\w+)"#) else {
                return (None, None);
            };
            (re.captures(text).map(|cap| cap[1].to_string()), None)
        }
        KeyMode::TypedBus => {
            let Ok(re) = Regex::new(r#"new\s+([A-Z]\w*)$"#) else {
                return (None, None);
            };
            (re.captures(text).map(|cap| cap[1].to_string()), Some(true))
        }
        KeyMode::Selector => {
            let Ok(re) = Regex::new(r#"#selector\s*\(\s*([\w.]+)"#) else {
                return (None, None);
            };
            let key = re
                .captures(text)
                .map(|cap| cap[1].split('.').next_back().unwrap_or(&cap[1]).to_string());
            (key, None)
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn push_match(
    out: &mut Vec<BoundaryMatch>,
    seen: &mut HashMap<String, usize>,
    form: &str,
    label: &str,
    original: &str,
    index: usize,
    file_start_line: u64,
    key: Option<String>,
    key_is_type: Option<bool>,
) {
    // 同类同 key 的站点合并成 `more_sites`，保留首个 snippet 作为代表。
    let dedupe_key = format!("{}|{}", form, key.as_deref().unwrap_or(""));
    if let Some(prior) = seen.get(&dedupe_key).copied() {
        let more = out[prior].more_sites.unwrap_or(0) + 1;
        out[prior].more_sites = Some(more);
        return;
    }
    let line = file_start_line + count_newlines(original, index) as u64;
    let match_ = BoundaryMatch {
        form: form.to_string(),
        label: label.to_string(),
        snippet: snippet_around(original, index),
        line,
        key,
        key_is_type,
        more_sites: None,
    };
    seen.insert(dedupe_key, out.len());
    out.push(match_);
}

fn single_string_literal(text: &str) -> Option<String> {
    let mut found = None;
    for quote in ['\'', '"', '`'] {
        let Some(start) = text.find(quote) else {
            continue;
        };
        let rest = &text[start + quote.len_utf8()..];
        let Some(end_rel) = rest.find(quote) else {
            continue;
        };
        if rest[end_rel + quote.len_utf8()..]
            .chars()
            .any(|c| c == '\'' || c == '"' || c == '`')
        {
            return None;
        }
        let inner = &rest[..end_rel];
        if (2..=64).contains(&inner.len())
            && inner
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | ':' | '-'))
        {
            found = Some(inner.to_string());
        }
    }
    found
}

fn match_balanced_paren(text: &str, open: usize) -> isize {
    let bytes = text.as_bytes();
    let mut depth = 0isize;
    let end = (open + MAX_GETATTR_ARGS).min(bytes.len());
    for (i, b) in bytes.iter().enumerate().take(end).skip(open) {
        if *b == b'(' {
            depth += 1;
        } else if *b == b')' {
            depth -= 1;
            if depth == 0 {
                return i as isize;
            }
        }
    }
    -1
}

fn count_newlines(text: &str, end: usize) -> usize {
    text.as_bytes()
        .iter()
        .take(end.min(text.len()))
        .filter(|b| **b == b'\n')
        .count()
}

fn snippet_around(text: &str, index: usize) -> String {
    let start = text[..index.min(text.len())]
        .rfind('\n')
        .map(|idx| idx + 1)
        .unwrap_or(0);
    let end = text[index.min(text.len())..]
        .find('\n')
        .map(|idx| index + idx)
        .unwrap_or(text.len());
    let line = text[start..end].trim();
    if line.len() > 120 {
        format!("{}...", &line[..117])
    } else {
        line.to_string()
    }
}

fn safe_slice(text: &str, start: usize, end: usize) -> &str {
    let start = clamp_to_boundary(text, start.min(text.len()));
    let end = clamp_to_boundary(text, end.min(text.len()));
    if start <= end { &text[start..end] } else { "" }
}

fn clamp_to_boundary(text: &str, mut idx: usize) -> usize {
    while idx > 0 && !text.is_char_boundary(idx) {
        idx -= 1;
    }
    idx
}

fn comment_lang(language: &str) -> Option<CommentLang> {
    match language {
        "python" => Some(CommentLang::Python),
        "ruby" => Some(CommentLang::Ruby),
        "rust" => Some(CommentLang::Rust),
        "php" => Some(CommentLang::Php),
        "go" => Some(CommentLang::Go),
        "javascript" | "jsx" => Some(CommentLang::JavaScript),
        "typescript" | "tsx" | "vue" | "svelte" | "astro" => Some(CommentLang::TypeScript),
        "java" | "kotlin" | "scala" | "dart" => Some(CommentLang::Java),
        "csharp" => Some(CommentLang::CSharp),
        "swift" => Some(CommentLang::Swift),
        "c" | "cpp" | "objc" | "objcpp" => Some(CommentLang::Java),
        _ => None,
    }
}

pub fn form_labels() -> HashSet<&'static str> {
    FORMS.iter().map(|form| form.form).collect()
}
