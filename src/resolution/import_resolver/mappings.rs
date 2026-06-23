//! Import statement extraction.
//!
//! 各语言前端在这里被归一成 `ImportMapping`：本地名、导出名、来源路径、
//! default/namespace 标记。后续解析只消费这张小表。

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use regex::Regex;

use crate::resolution::types::ImportMapping;
use crate::types::Language;

use super::cpp::clear_cpp_include_dir_cache;

static IMPORT_MAPPING_CACHE: LazyLock<Mutex<HashMap<String, Vec<ImportMapping>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub fn extract_import_mappings(
    _file_path: &str,
    content: &str,
    language: Language,
) -> Vec<ImportMapping> {
    // SFC/模板类语言复用 JS import 规则，因为脚本块已经在上游提取为同一份文本。
    match language {
        Language::TypeScript | Language::JavaScript | Language::Tsx | Language::Jsx => {
            extract_js_imports(content)
        }
        Language::Svelte | Language::Vue | Language::Astro => extract_js_imports(content),
        Language::Python => extract_python_imports(content),
        Language::Go => extract_go_imports(content),
        Language::Rust => extract_rust_imports(content),
        Language::Java | Language::Kotlin => extract_java_imports(content),
        Language::Php => extract_php_imports(content),
        Language::C | Language::Cpp => extract_cpp_imports(content),
        _ => Vec::new(),
    }
}

fn mapping(
    local_name: impl Into<String>,
    exported_name: impl Into<String>,
    source: impl Into<String>,
    is_default: bool,
    is_namespace: bool,
) -> ImportMapping {
    ImportMapping {
        local_name: local_name.into(),
        exported_name: exported_name.into(),
        source: source.into(),
        is_default,
        is_namespace,
        resolved_path: None,
    }
}

fn extract_js_imports(content: &str) -> Vec<ImportMapping> {
    static IMPORT_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"import\s+(?:(\w+)\s*,?\s*)?(?:\{([^}]+)\})?\s*(?:(\*)\s+as\s+(\w+))?\s*from\s*['"]([^'"]+)['"]"#).unwrap()
    });
    static REQUIRE_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"(?:const|let|var)\s+(?:(\w+)|\{([^}]+)\})\s*=\s*require\(['"]([^'"]+)['"]\)"#)
            .unwrap()
    });
    let mut out = Vec::new();
    for cap in IMPORT_RE.captures_iter(content) {
        let source = cap.get(5).map(|m| m.as_str()).unwrap_or("");
        if let Some(default_import) = cap.get(1).map(|m| m.as_str()).filter(|s| !s.is_empty()) {
            out.push(mapping(default_import, "default", source, true, false));
        }
        if let Some(named) = cap.get(2).map(|m| m.as_str()) {
            for item in named
                .split(',')
                .map(str::trim)
                .filter(|item| !item.is_empty())
            {
                if let Some((orig, alias)) = item.split_once(" as ") {
                    out.push(mapping(alias.trim(), orig.trim(), source, false, false));
                } else {
                    out.push(mapping(item, item, source, false, false));
                }
            }
        }
        if cap.get(3).map(|m| m.as_str()) == Some("*")
            && let Some(ns) = cap.get(4).map(|m| m.as_str())
        {
            out.push(mapping(ns, "*", source, false, true));
        }
    }
    for cap in REQUIRE_RE.captures_iter(content) {
        let source = cap.get(3).map(|m| m.as_str()).unwrap_or("");
        if let Some(default_name) = cap.get(1).map(|m| m.as_str()).filter(|s| !s.is_empty()) {
            out.push(mapping(default_name, "default", source, true, false));
        }
        if let Some(destructured) = cap.get(2).map(|m| m.as_str()) {
            for item in destructured
                .split(',')
                .map(str::trim)
                .filter(|item| !item.is_empty())
            {
                if let Some((orig, alias)) = item.split_once(':') {
                    out.push(mapping(alias.trim(), orig.trim(), source, false, false));
                } else {
                    out.push(mapping(item, item, source, false, false));
                }
            }
        }
    }
    out
}

fn extract_python_imports(content: &str) -> Vec<ImportMapping> {
    static FROM_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"from\s+([\w.]+)\s+import\s+([^#\n]+)"#).unwrap());
    static IMPORT_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"(?m)^import\s+([\w.]+)(?:\s+as\s+(\w+))?"#).unwrap());
    let mut out = Vec::new();
    for cap in FROM_RE.captures_iter(content) {
        let source = cap.get(1).map(|m| m.as_str()).unwrap_or("");
        for item in cap.get(2).map(|m| m.as_str()).unwrap_or("").split(',') {
            let item = item.trim();
            if item.is_empty() || item == "*" {
                continue;
            }
            if let Some((orig, alias)) = item.split_once(" as ") {
                out.push(mapping(alias.trim(), orig.trim(), source, false, false));
            } else {
                out.push(mapping(item, item, source, false, false));
            }
        }
    }
    for cap in IMPORT_RE.captures_iter(content) {
        let source = cap.get(1).map(|m| m.as_str()).unwrap_or("");
        let local = cap
            .get(2)
            .map(|m| m.as_str())
            .unwrap_or_else(|| source.rsplit('.').next().unwrap_or(source));
        out.push(mapping(local, "*", source, false, true));
    }
    out
}

fn extract_go_imports(content: &str) -> Vec<ImportMapping> {
    static SINGLE_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"import\s+(?:(\w+)\s+)?["']([^"']+)["']"#).unwrap());
    static BLOCK_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"(?s)import\s*\(\s*([^)]+)\s*\)"#).unwrap());
    static LINE_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"(?:(\w+)\s+)?["']([^"']+)["']"#).unwrap());
    let mut out = Vec::new();
    for cap in SINGLE_RE.captures_iter(content) {
        let source = cap.get(2).map(|m| m.as_str()).unwrap_or("");
        let package_name = source.rsplit('/').next().unwrap_or(source);
        out.push(mapping(
            cap.get(1).map(|m| m.as_str()).unwrap_or(package_name),
            "*",
            source,
            false,
            true,
        ));
    }
    for block in BLOCK_RE.captures_iter(content).filter_map(|cap| cap.get(1)) {
        for cap in LINE_RE.captures_iter(block.as_str()) {
            let source = cap.get(2).map(|m| m.as_str()).unwrap_or("");
            let package_name = source.rsplit('/').next().unwrap_or(source);
            out.push(mapping(
                cap.get(1).map(|m| m.as_str()).unwrap_or(package_name),
                "*",
                source,
                false,
                true,
            ));
        }
    }
    out
}

fn extract_rust_imports(content: &str) -> Vec<ImportMapping> {
    static USE_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"(?m)^\s*(?:pub(?:\([^)]*\))?\s+)?use\s+([^;]+);"#).unwrap());
    let mut out = Vec::new();
    for cap in USE_RE.captures_iter(content) {
        if let Some(spec) = cap.get(1).map(|m| m.as_str().trim()) {
            push_rust_use_mappings(spec, &mut out);
        }
    }
    out
}

fn push_rust_use_mappings(spec: &str, out: &mut Vec<ImportMapping>) {
    // Rust `use a::{b, c::{d as e}}` 是递归结构；先展开花括号，再把每个叶子
    // 转成统一的 source/leaf mapping。
    let spec = spec.trim();
    if spec.is_empty() {
        return;
    }

    if let Some((open, close)) = rust_use_brace_span(spec) {
        let prefix = spec[..open].trim().trim_end_matches("::");
        let body = &spec[open + 1..close];
        for item in split_rust_use_items(body) {
            let item = item.trim();
            if item.is_empty() {
                continue;
            }
            if item == "self" {
                if let Some((source, leaf)) = rust_parent_and_leaf(prefix) {
                    out.push(mapping(leaf, leaf, source, false, false));
                }
                continue;
            }
            if item == "*" {
                let local = prefix.rsplit("::").next().unwrap_or(prefix);
                out.push(mapping(local, "*", prefix, false, true));
                continue;
            }
            let full = if prefix.is_empty() {
                item.to_string()
            } else {
                format!("{prefix}::{item}")
            };
            push_rust_use_mappings(&full, out);
        }
        return;
    }

    let (raw_path, alias) = spec
        .split_once(" as ")
        .map(|(path, alias)| (path.trim(), Some(alias.trim())))
        .unwrap_or((spec, None));
    let path = raw_path.trim();
    if path.is_empty() {
        return;
    }
    if let Some(module) = path.strip_suffix("::*") {
        let local = module.rsplit("::").next().unwrap_or(module);
        out.push(mapping(local, "*", module, false, true));
        return;
    }
    let Some((source, leaf)) = rust_parent_and_leaf(path) else {
        return;
    };
    out.push(mapping(alias.unwrap_or(leaf), leaf, source, false, false));
}

fn rust_use_brace_span(spec: &str) -> Option<(usize, usize)> {
    let mut depth = 0usize;
    let mut open = None;
    for (idx, ch) in spec.char_indices() {
        match ch {
            '{' => {
                if depth == 0 {
                    open = Some(idx);
                }
                depth += 1;
            }
            '}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return open.map(|open| (open, idx));
                }
            }
            _ => {}
        }
    }
    None
}

fn split_rust_use_items(body: &str) -> Vec<&str> {
    // 只在顶层逗号切分，保留内层 `{...}` 给下一轮递归处理。
    let mut items = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (idx, ch) in body.char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                items.push(&body[start..idx]);
                start = idx + 1;
            }
            _ => {}
        }
    }
    items.push(&body[start..]);
    items
}

fn rust_parent_and_leaf(path: &str) -> Option<(&str, &str)> {
    let (source, leaf) = path.rsplit_once("::")?;
    (!source.is_empty() && !leaf.is_empty()).then_some((source, leaf))
}

fn extract_java_imports(content: &str) -> Vec<ImportMapping> {
    static COMMENT_BLOCK_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"(?s)/\*.*?\*/"#).unwrap());
    static COMMENT_LINE_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"//[^\n]*"#).unwrap());
    static IMPORT_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"(?m)^\s*import\s+(?:static\s+)?([\w.]+(?:\.\*)?)\s*;?\s*$"#).unwrap()
    });
    let without_block_comments = COMMENT_BLOCK_RE.replace_all(content, "");
    let stripped = COMMENT_LINE_RE.replace_all(&without_block_comments, "");
    let mut out = Vec::new();
    for cap in IMPORT_RE.captures_iter(&stripped) {
        let fqn = cap.get(1).map(|m| m.as_str()).unwrap_or("");
        if fqn.ends_with(".*") {
            continue;
        }
        if let Some(local) = fqn.rsplit('.').next().filter(|s| !s.is_empty()) {
            out.push(mapping(local, local, fqn, false, false));
        }
    }
    out
}

fn extract_php_imports(content: &str) -> Vec<ImportMapping> {
    static USE_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"use\s+([\w\\]+)(?:\s+as\s+(\w+))?;"#).unwrap());
    let mut out = Vec::new();
    for cap in USE_RE.captures_iter(content) {
        let full_path = cap.get(1).map(|m| m.as_str()).unwrap_or("");
        let class_name = full_path.rsplit('\\').next().unwrap_or(full_path);
        out.push(mapping(
            cap.get(2).map(|m| m.as_str()).unwrap_or(class_name),
            class_name,
            full_path,
            false,
            false,
        ));
    }
    out
}

fn extract_cpp_imports(content: &str) -> Vec<ImportMapping> {
    static INCLUDE_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"(?m)^\s*#\s*include\s+[<"]([^>"]+)[>"]"#).unwrap());
    let mut out = Vec::new();
    for cap in INCLUDE_RE.captures_iter(content) {
        let module_path = cap.get(1).map(|m| m.as_str()).unwrap_or("");
        let basename = module_path
            .rsplit('/')
            .next()
            .unwrap_or(module_path)
            .trim_end_matches(".hpp")
            .trim_end_matches(".hxx")
            .trim_end_matches(".hh")
            .trim_end_matches(".inl")
            .trim_end_matches(".ipp")
            .trim_end_matches(".cxx")
            .trim_end_matches(".cpp")
            .trim_end_matches(".cc")
            .trim_end_matches(".h");
        out.push(mapping(
            if basename.is_empty() {
                module_path
            } else {
                basename
            },
            "*",
            module_path,
            false,
            true,
        ));
    }
    out
}

pub fn clear_import_mapping_cache() {
    // include 路径也是 import 解析的一部分；清 import cache 时同步清掉 C++ 缓存。
    IMPORT_MAPPING_CACHE
        .lock()
        .expect("import mapping cache poisoned")
        .clear();
    clear_cpp_include_dir_cache();
}
