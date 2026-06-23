//! Rust translation of `src/extraction/languages/java.ts`.

use crate::extraction::tree_sitter_helpers::{get_child_by_field, get_node_text};
use crate::extraction::tree_sitter_types::{ImportInfo, LanguageExtractor, Visibility};
use crate::web_tree_sitter::SyntaxNode;

pub struct JavaExtractor;

pub const JAVA_EXTRACTOR: JavaExtractor = JavaExtractor;

pub fn java_extractor() -> &'static JavaExtractor {
    &JAVA_EXTRACTOR
}

const JAVA_NON_CLASS_RETURN_NODES: &[&str] = &[
    "void_type",
    "integral_type",
    "floating_point_type",
    "boolean_type",
];

fn extract_java_return_type(node: &SyntaxNode, source: &str) -> Option<String> {
    let type_node = get_child_by_field(node, "type")?;
    // 返回类型只保留类名，供后续链式调用和 receiver 推断使用；基础类型不会形成有用图边。
    if JAVA_NON_CLASS_RETURN_NODES.contains(&type_node.type_name().as_str())
        || type_node.type_name() == "array_type"
    {
        return None;
    }
    let raw = strip_angle_generics(get_node_text(type_node, source).trim());
    let last = raw.split('.').next_back()?.trim();
    is_identifier(last).then(|| last.to_string())
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

impl LanguageExtractor for JavaExtractor {
    fn function_types(&self) -> &'static [&'static str] {
        &[]
    }

    fn class_types(&self) -> &'static [&'static str] {
        &["class_declaration"]
    }

    fn method_types(&self) -> &'static [&'static str] {
        &["method_declaration", "constructor_declaration"]
    }

    fn interface_types(&self) -> &'static [&'static str] {
        &["interface_declaration", "annotation_type_declaration"]
    }

    fn struct_types(&self) -> &'static [&'static str] {
        &[]
    }

    fn enum_types(&self) -> &'static [&'static str] {
        &["enum_declaration"]
    }

    fn enum_member_types(&self) -> &'static [&'static str] {
        &["enum_constant"]
    }

    fn type_alias_types(&self) -> &'static [&'static str] {
        &[]
    }

    fn import_types(&self) -> &'static [&'static str] {
        &["import_declaration"]
    }

    fn call_types(&self) -> &'static [&'static str] {
        &["method_invocation"]
    }

    fn variable_types(&self) -> &'static [&'static str] {
        &["local_variable_declaration"]
    }

    fn field_types(&self) -> &'static [&'static str] {
        &["field_declaration"]
    }

    fn name_field(&self) -> &'static str {
        "name"
    }

    fn body_field(&self) -> &'static str {
        "body"
    }

    fn params_field(&self) -> &'static str {
        "parameters"
    }

    fn return_field(&self) -> Option<&'static str> {
        Some("type")
    }

    fn get_return_type(&self, node: &SyntaxNode, source: &str) -> Option<String> {
        extract_java_return_type(node, source)
    }

    fn get_signature(&self, node: &SyntaxNode, source: &str) -> Option<String> {
        // 构造器没有 return type，方法有；签名统一落成 `Return (params)` 或仅 `(params)`。
        let params = get_child_by_field(node, "parameters")?;
        let params_text = get_node_text(params, source);
        let sig = get_child_by_field(node, "type")
            .map(|return_type| format!("{} {}", get_node_text(return_type, source), params_text))
            .unwrap_or(params_text);
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
                if text.contains("private") {
                    return Some(Visibility::Private);
                }
                if text.contains("protected") {
                    return Some(Visibility::Protected);
                }
            }
        }
        None
    }

    fn is_static(&self, node: &SyntaxNode) -> bool {
        (0..node.child_count()).any(|i| {
            node.child(i)
                .is_some_and(|c| c.type_name() == "modifiers" && c.text().contains("static"))
        })
    }

    fn is_const(&self, node: &SyntaxNode) -> bool {
        for i in 0..node.child_count() {
            let Some(child) = node.child(i) else {
                continue;
            };
            if child.type_name() == "modifiers" {
                let text = child.text();
                return text.contains("static") && text.contains("final");
            }
        }
        false
    }

    fn extract_import(&self, node: &SyntaxNode, source: &str) -> Option<ImportInfo> {
        // 只记录 scoped_identifier；static/wildcard 的具体成员解析交给 import resolver。
        let scoped_id = node
            .named_children()
            .into_iter()
            .find(|c| c.type_name() == "scoped_identifier")?;
        Some(ImportInfo {
            module_name: get_node_text(&scoped_id, source),
            signature: get_node_text(node, source).trim().to_string(),
            handled_refs: false,
        })
    }

    fn package_types(&self) -> &'static [&'static str] {
        &["package_declaration"]
    }

    fn extract_package(&self, node: &SyntaxNode, source: &str) -> Option<String> {
        // package_declaration 可能只有 identifier，也可能是 scoped_identifier。
        node.named_children()
            .into_iter()
            .find(|c| c.type_name() == "scoped_identifier" || c.type_name() == "identifier")
            .map(|id| get_node_text(&id, source).trim().to_string())
    }
}
