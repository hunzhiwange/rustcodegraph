//! Swift ↔ Objective-C selector name helpers.
//!
//! 中文维护提示：这些 helper 只做命名桥接，不尝试完整实现 Swift ABI。它们用于把
//! `@objc` 暴露的方法、init 和属性访问器连到 Objective-C selector 形状。

use std::collections::HashSet;

fn cap_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn lower_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_lowercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

pub fn objc_selector_for_swift_method(
    base_name: &str,
    external_labels: &[Option<String>],
    explicit_objc_name: Option<&str>,
) -> Option<String> {
    if base_name.is_empty() {
        return None;
    }
    if let Some(name) = explicit_objc_name.filter(|name| !name.is_empty()) {
        // 显式 `@objc(name:)` 优先级最高，后续 Swift 参数标签规则都不再参与。
        return Some(name.to_string());
    }
    if external_labels.is_empty() {
        return Some(base_name.to_string());
    }

    let first = external_labels.first().and_then(|label| label.as_deref());
    // Swift 第一个外部标签缺省时，ObjC selector 直接用 baseName:；有标签时按
    // Cocoa 风格拼成 baseNameWithLabel:。
    let first_keyword = match first {
        None | Some("_") | Some("") => format!("{base_name}:"),
        Some(label) => format!("{base_name}With{}:", cap_first(label)),
    };
    let rest = external_labels
        .iter()
        .skip(1)
        .map(|label| format!("{}:", label.as_deref().unwrap_or("")))
        .collect::<String>();
    Some(first_keyword + &rest)
}

pub fn objc_selector_for_swift_init(
    external_labels: &[Option<String>],
    internal_names: &[String],
    explicit_objc_name: Option<&str>,
) -> Option<String> {
    if let Some(name) = explicit_objc_name.filter(|name| !name.is_empty()) {
        return Some(name.to_string());
    }
    if external_labels.is_empty() {
        return Some("init".to_string());
    }

    let first_ext = external_labels.first().and_then(|label| label.as_deref());
    let first_int = internal_names.first().map(String::as_str);
    // init 的第一个 `_` 外部标签会退回内部参数名，这是 Swift 到 ObjC selector
    // 的一个常见特殊点。
    let first_label = match first_ext {
        None | Some("_") | Some("") => first_int,
        Some(label) => Some(label),
    }?;
    if first_label.is_empty() {
        return None;
    }

    let rest = external_labels
        .iter()
        .skip(1)
        .enumerate()
        .map(|(idx, label)| {
            let internal = internal_names
                .get(idx + 1)
                .map(String::as_str)
                .unwrap_or("");
            let name = match label.as_deref() {
                Some(label) if label != "_" && !label.is_empty() => label,
                _ => internal,
            };
            format!("{name}:")
        })
        .collect::<String>();

    Some(format!("initWith{}:{rest}", cap_first(first_label)))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjcAccessors {
    pub getter: String,
    pub setter: String,
}

pub fn objc_accessors_for_swift_property(
    swift_name: &str,
    explicit_objc_name: Option<&str>,
) -> Option<ObjcAccessors> {
    if swift_name.is_empty() {
        return None;
    }
    let getter = explicit_objc_name
        .filter(|name| !name.is_empty())
        .unwrap_or(swift_name)
        .to_string();
    Some(ObjcAccessors {
        setter: format!("set{}:", cap_first(&getter)),
        getter,
    })
}

pub fn swift_base_names_for_objc_selector(selector: &str) -> Vec<String> {
    if selector.is_empty() {
        return Vec::new();
    }
    let trimmed = selector.trim_end_matches(':');
    let keywords = trimmed.split(':').collect::<Vec<_>>();
    let Some(first_keyword) = keywords.first().copied().filter(|s| !s.is_empty()) else {
        return Vec::new();
    };

    let mut candidates = HashSet::new();
    candidates.insert(first_keyword.to_string());

    if first_keyword.starts_with("initWith") {
        candidates.insert("init".to_string());
    }

    for prep in [
        "With", "For", "By", "In", "On", "At", "From", "To", "Of", "As",
    ] {
        // 从 `doThingWithValue:` 反推 Swift base name `doThing`，用于 ObjC 调用点
        // 回连 Swift 暴露方法。
        if let Some(idx) = first_keyword.find(prep)
            && idx > 0
            && first_keyword[..idx]
                .chars()
                .next()
                .map(|ch| ch.is_ascii_lowercase())
                .unwrap_or(false)
            && first_keyword[idx + prep.len()..]
                .chars()
                .next()
                .map(|ch| ch.is_ascii_uppercase())
                .unwrap_or(false)
        {
            candidates.insert(first_keyword[..idx].to_string());
        }
    }

    if keywords.len() == 1
        && selector.ends_with(':')
        && first_keyword.starts_with("set")
        && first_keyword
            .chars()
            .nth(3)
            .map(|ch| ch.is_ascii_uppercase())
            .unwrap_or(false)
    {
        // 单参数 `setFoo:` 同时可能是属性 setter，补出 Swift 属性名 `foo`。
        let prop_name = lower_first(&first_keyword[3..]);
        if !prop_name.is_empty() {
            candidates.insert(prop_name);
        }
    }

    let mut out = candidates.into_iter().collect::<Vec<_>>();
    out.sort();
    out
}

pub fn detect_explicit_objc_name(source_slice: &str) -> Option<String> {
    let marker = source_slice.find("@objc")?;
    let rest = &source_slice[marker + "@objc".len()..];
    let open = rest.find('(')?;
    let after_open = &rest[open + 1..];
    let close = after_open.find(')')?;
    let name = after_open[..close].trim();
    if name.is_empty() || name.chars().any(char::is_whitespace) {
        None
    } else {
        Some(name.to_string())
    }
}

pub fn is_objc_exposed(source_slice: &str) -> bool {
    !source_slice.contains("@nonobjc") && source_slice.contains("@objc")
}
