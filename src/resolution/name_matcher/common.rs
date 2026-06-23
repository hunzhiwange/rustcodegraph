//! name matcher 之间共享的语言家族、字符串拆分和打分工具。
//!
//! 这些 helper 不直接建边，但决定候选能否跨语言家族、同名候选如何按路径接近度
//! 排序，因此会影响 resolver 的误连/漏连边界。

use crate::resolution::types::{ResolvedBy, ResolvedRef, UnresolvedRef, language_name};
use crate::types::{Language, Node, ReferenceKind};

const LANGUAGE_FAMILY: &[(&str, &str)] = &[
    ("java", "jvm"),
    ("kotlin", "jvm"),
    ("scala", "jvm"),
    ("swift", "apple"),
    ("objc", "apple"),
    ("typescript", "web"),
    ("tsx", "web"),
    ("javascript", "web"),
    ("jsx", "web"),
    ("c", "c"),
    ("cpp", "c"),
    ("csharp", "dotnet"),
    ("razor", "dotnet"),
];

pub fn same_language_family(a: Language, b: Language) -> bool {
    if a == b {
        return true;
    }
    let fa = family(language_name(a));
    fa.is_some() && fa == family(language_name(b))
}

pub fn is_known_language_family(lang: Language) -> bool {
    family(language_name(lang)).is_some()
}

pub fn crosses_known_family(a: Language, b: Language) -> bool {
    is_known_language_family(a) && is_known_language_family(b) && !same_language_family(a, b)
}

fn family(lang: &str) -> Option<&'static str> {
    LANGUAGE_FAMILY
        .iter()
        .find_map(|(key, family)| (*key == lang).then_some(*family))
}

pub(super) fn apply_language_gate(candidates: Vec<Node>, reference: &UnresolvedRef) -> Vec<Node> {
    // 普通引用必须留在同一语言家族；imports 允许指向 Unknown，但不允许 web/JVM
    // 等已知家族互串，防止同名符号跨生态误连。
    match reference.reference_kind {
        ReferenceKind::References | ReferenceKind::FunctionRef => candidates
            .into_iter()
            .filter(|node| same_language_family(node.language, reference.language))
            .collect(),
        ReferenceKind::Imports => candidates
            .into_iter()
            .filter(|node| !crosses_known_family(node.language, reference.language))
            .collect(),
        _ => candidates,
    }
}

pub(super) fn compute_path_proximity(file_path1: &str, file_path2: &str) -> i32 {
    // 路径前缀相同越多，说明符号越可能属于同一模块；分值封顶是为了不压过
    // 语言和引用 kind 这类更强信号。
    let dir1 = file_path1.split('/').collect::<Vec<_>>();
    let dir2 = file_path2.split('/').collect::<Vec<_>>();
    let max = dir1
        .len()
        .saturating_sub(1)
        .min(dir2.len().saturating_sub(1));
    let mut shared = 0;
    for i in 0..max {
        if dir1[i] == dir2[i] {
            shared += 1;
        } else {
            break;
        }
    }
    (shared * 15).min(80)
}

pub(super) fn split_chain_shape(value: &str) -> Option<(&str, &str)> {
    let idx = value.rfind("().")?;
    let inner = &value[..idx];
    let method = &value[idx + 3..];
    (!inner.is_empty() && !method.is_empty()).then_some((inner, method))
}

pub(super) fn split_camel_case(value: &str) -> Vec<String> {
    let mut out = String::new();
    let chars = value.chars().collect::<Vec<_>>();
    for (idx, ch) in chars.iter().enumerate() {
        if idx > 0 {
            let prev = chars[idx - 1];
            let next = chars.get(idx + 1).copied();
            if (prev.is_ascii_lowercase() && ch.is_ascii_uppercase())
                || (prev.is_ascii_uppercase()
                    && ch.is_ascii_uppercase()
                    && next.map(|n| n.is_ascii_lowercase()).unwrap_or(false))
            {
                out.push(' ');
            }
        }
        if ch.is_ascii_alphanumeric() {
            out.push(*ch);
        } else {
            out.push(' ');
        }
    }
    out.split_whitespace()
        .filter(|word| word.len() > 1)
        .map(ToOwned::to_owned)
        .collect()
}

pub(super) fn strip_angle_generics(input: &str) -> String {
    let mut out = String::new();
    let mut depth = 0usize;
    for ch in input.chars() {
        match ch {
            '<' => depth += 1,
            '>' => depth = depth.saturating_sub(1),
            _ if depth == 0 => out.push(ch),
            _ => {}
        }
    }
    out
}

pub(super) fn capitalize(input: &str) -> String {
    let mut chars = input.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

pub(super) fn resolved(
    reference: &UnresolvedRef,
    target_node_id: &str,
    confidence: f64,
    resolved_by: ResolvedBy,
) -> ResolvedRef {
    ResolvedRef {
        original: reference.clone(),
        target_node_id: target_node_id.to_string(),
        confidence,
        resolved_by,
    }
}
