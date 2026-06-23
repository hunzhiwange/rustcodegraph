//! Rust translation of `src/extraction/languages/swift.ts`.

use crate::extraction::tree_sitter_helpers::{get_child_by_field, get_node_text};
use crate::extraction::tree_sitter_types::{
    ClassClassification, ImportInfo, LanguageExtractor, Visibility,
};
use crate::web_tree_sitter::SyntaxNode;

pub struct SwiftExtractor;

pub const SWIFT_EXTRACTOR: SwiftExtractor = SwiftExtractor;

pub fn swift_extractor() -> &'static SwiftExtractor {
    &SWIFT_EXTRACTOR
}

fn extract_swift_return_type(node: &SyntaxNode, source: &str) -> Option<String> {
    // Swift grammar 没有稳定 return_type 字段时，从函数名之后扫描第一个 user_type/optional_type。
    let mut seen_name = false;
    for i in 0..node.named_child_count() {
        let Some(child) = node.named_child(i) else {
            continue;
        };
        if child.type_name() == "simple_identifier" && !seen_name {
            seen_name = true;
            continue;
        }
        if !seen_name {
            continue;
        }
        if child.type_name() == "function_body" {
            return None;
        }
        let type_node = if child.type_name() == "user_type" {
            Some(child.clone())
        } else if child.type_name() == "optional_type" {
            child
                .named_children()
                .into_iter()
                .find(|c| c.type_name() == "user_type")
        } else {
            None
        };
        if let Some(type_node) = type_node {
            let name = strip_angle_generics(get_node_text(&type_node, source).trim());
            let last = name.split('.').next_back()?.trim();
            if !is_identifier(last) || last == "Void" {
                return None;
            }
            return Some(last.to_string());
        }
    }
    None
}

fn strip_angle_generics(input: &str) -> String {
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

fn is_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some('_') | Some('A'..='Z') | Some('a'..='z'))
        && chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
}

impl LanguageExtractor for SwiftExtractor {
    fn function_types(&self) -> &'static [&'static str] {
        &["function_declaration"]
    }

    fn class_types(&self) -> &'static [&'static str] {
        &["class_declaration"]
    }

    fn method_types(&self) -> &'static [&'static str] {
        &["function_declaration"]
    }

    fn interface_types(&self) -> &'static [&'static str] {
        &["protocol_declaration"]
    }

    fn struct_types(&self) -> &'static [&'static str] {
        &["struct_declaration"]
    }

    fn enum_types(&self) -> &'static [&'static str] {
        &["enum_declaration"]
    }

    fn enum_member_types(&self) -> &'static [&'static str] {
        &["enum_entry"]
    }

    fn type_alias_types(&self) -> &'static [&'static str] {
        &["typealias_declaration"]
    }

    fn import_types(&self) -> &'static [&'static str] {
        &["import_declaration"]
    }

    fn call_types(&self) -> &'static [&'static str] {
        &["call_expression"]
    }

    fn variable_types(&self) -> &'static [&'static str] {
        &["property_declaration", "constant_declaration"]
    }

    fn name_field(&self) -> &'static str {
        "name"
    }

    fn body_field(&self) -> &'static str {
        "body"
    }

    fn params_field(&self) -> &'static str {
        "parameter"
    }

    fn return_field(&self) -> Option<&'static str> {
        Some("return_type")
    }

    fn get_return_type(&self, node: &SyntaxNode, source: &str) -> Option<String> {
        extract_swift_return_type(node, source)
    }

    fn resolve_name(&self, node: &SyntaxNode, source: &str) -> Option<String> {
        if node.type_name() == "function_declaration"
            || node.type_name() == "protocol_function_declaration"
        {
            // 协议函数声明和普通函数都需要跳过 `func` token 后取第一个 simple_identifier。
            let mut seen_func = false;
            for child in node.children() {
                let child_type = child.type_name();
                if child_type == "func" {
                    seen_func = true;
                    continue;
                }
                if seen_func && child_type == "simple_identifier" {
                    return Some(get_node_text(&child, source));
                }
            }
        }
        if node.type_name() != "class_declaration" {
            return None;
        }
        // 嵌套/限定 class name 只把最后一段作为节点名，qualified 信息由外层作用域补齐。
        let name_node = get_child_by_field(node, "name")?;
        if name_node.type_name() != "user_type" {
            return None;
        }
        let ids = name_node
            .named_children()
            .into_iter()
            .filter(|c| c.type_name() == "type_identifier")
            .collect::<Vec<_>>();
        (ids.len() > 1).then(|| get_node_text(ids.last().unwrap(), source))
    }

    fn get_signature(&self, node: &SyntaxNode, source: &str) -> Option<String> {
        let params = get_child_by_field(node, "parameter")?;
        let mut sig = get_node_text(params, source);
        if let Some(return_type) = get_child_by_field(node, "return_type") {
            sig.push_str(" -> ");
            sig.push_str(&get_node_text(return_type, source));
        }
        Some(sig)
    }

    fn get_visibility(&self, node: &SyntaxNode) -> Option<Visibility> {
        for i in 0..node.child_count() {
            let Some(child) = node.child(i) else {
                continue;
            };
            if child.type_name() == "modifiers" {
                let text = child.text();
                if text.contains("public") {
                    return Some(Visibility::Public);
                }
                if text.contains("private") || text.contains("fileprivate") {
                    return Some(Visibility::Private);
                }
                if text.contains("internal") {
                    return Some(Visibility::Internal);
                }
            }
        }
        // Swift 顶层默认 internal，明确写出可避免可见性查询得到 None。
        Some(Visibility::Internal)
    }

    fn is_static(&self, node: &SyntaxNode) -> bool {
        (0..node.child_count()).any(|i| {
            node.child(i).is_some_and(|c| {
                c.type_name() == "modifiers"
                    && (c.text().contains("static") || c.text().contains("class"))
            })
        })
    }

    fn classify_class_node(&self, node: &SyntaxNode) -> ClassClassification {
        for i in 0..node.child_count() {
            let Some(child) = node.child(i) else {
                continue;
            };
            if child.type_name() == "struct" {
                return ClassClassification::Struct;
            }
            if child.type_name() == "enum" {
                return ClassClassification::Enum;
            }
        }
        ClassClassification::Class
    }

    fn is_async(&self, node: &SyntaxNode) -> bool {
        (0..node.child_count()).any(|i| {
            node.child(i)
                .is_some_and(|c| c.type_name() == "modifiers" && c.text().contains("async"))
        })
    }

    fn extract_import(&self, node: &SyntaxNode, source: &str) -> Option<ImportInfo> {
        let identifier = node
            .named_children()
            .into_iter()
            .find(|c| c.type_name() == "identifier")?;
        Some(ImportInfo {
            module_name: get_node_text(&identifier, source),
            signature: get_node_text(node, source).trim().to_string(),
            handled_refs: false,
        })
    }
}
