//! Rust translation of `src/extraction/languages/rust.ts`.

use crate::extraction::tree_sitter_helpers::{get_child_by_field, get_node_text};
use crate::extraction::tree_sitter_types::{ImportInfo, LanguageExtractor, Visibility};
use crate::types::NodeKind;
use crate::web_tree_sitter::SyntaxNode;

pub struct RustExtractor;

pub const RUST_EXTRACTOR: RustExtractor = RustExtractor;

pub fn rust_extractor() -> &'static RustExtractor {
    &RUST_EXTRACTOR
}

fn extract_rust_return_type(node: &SyntaxNode, source: &str) -> Option<String> {
    let mut rt = get_child_by_field(node, "return_type")?.clone();
    // `&T` / `&mut T` 的接收者推断仍应看到 T，而不是 reference_type。
    if rt.type_name() == "reference_type"
        && let Some(inner) = rt.named_children().into_iter().find(|c| {
            c.type_name() == "type_identifier"
                || c.type_name() == "scoped_type_identifier"
                || c.type_name() == "generic_type"
        })
    {
        rt = inner;
    }
    if matches!(
        rt.type_name().as_str(),
        "primitive_type" | "unit_type" | "tuple_type"
    ) {
        return None;
    }
    let text = strip_angle_generics(get_node_text(&rt, source).trim());
    let last = text.split("::").last()?.trim();
    if !is_identifier(last) {
        return None;
    }
    // Self 需要和后续 receiver/self 解析约定一致。
    Some(if last == "Self" { "self" } else { last }.to_string())
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

fn root_module(scoped_node: &SyntaxNode, source: &str) -> String {
    // `use a::b::{c,d}` 先记录根模块 a，具体展开留给 import resolver 结合文件系统处理。
    let Some(first_child) = scoped_node.named_child(0) else {
        return get_node_text(scoped_node, source);
    };
    match first_child.type_name().as_str() {
        "identifier" | "crate" | "super" | "self" => get_node_text(first_child, source),
        "scoped_identifier" => root_module(first_child, source),
        _ => get_node_text(first_child, source),
    }
}

impl LanguageExtractor for RustExtractor {
    fn function_types(&self) -> &'static [&'static str] {
        &["function_item", "function_signature_item"]
    }

    fn class_types(&self) -> &'static [&'static str] {
        &[]
    }

    fn method_types(&self) -> &'static [&'static str] {
        &["function_item", "function_signature_item"]
    }

    fn interface_types(&self) -> &'static [&'static str] {
        &["trait_item"]
    }

    fn interface_kind(&self) -> Option<NodeKind> {
        Some(NodeKind::Trait)
    }

    fn struct_types(&self) -> &'static [&'static str] {
        &["struct_item"]
    }

    fn enum_types(&self) -> &'static [&'static str] {
        &["enum_item"]
    }

    fn enum_member_types(&self) -> &'static [&'static str] {
        &["enum_variant"]
    }

    fn type_alias_types(&self) -> &'static [&'static str] {
        &["type_item"]
    }

    fn import_types(&self) -> &'static [&'static str] {
        &["use_declaration"]
    }

    fn call_types(&self) -> &'static [&'static str] {
        &["call_expression"]
    }

    fn variable_types(&self) -> &'static [&'static str] {
        &["let_declaration", "const_item", "static_item"]
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
        Some("return_type")
    }

    fn get_return_type(&self, node: &SyntaxNode, source: &str) -> Option<String> {
        extract_rust_return_type(node, source)
    }

    fn get_signature(&self, node: &SyntaxNode, source: &str) -> Option<String> {
        let params = get_child_by_field(node, "parameters")?;
        let mut sig = get_node_text(params, source);
        if let Some(return_type) = get_child_by_field(node, "return_type") {
            sig.push_str(" -> ");
            sig.push_str(&get_node_text(return_type, source));
        }
        Some(sig)
    }

    fn is_async(&self, node: &SyntaxNode) -> bool {
        (0..node.child_count()).any(|i| node.child(i).is_some_and(|c| c.type_name() == "async"))
    }

    fn get_visibility(&self, node: &SyntaxNode) -> Option<Visibility> {
        for i in 0..node.child_count() {
            let Some(child) = node.child(i) else {
                continue;
            };
            if child.type_name() == "visibility_modifier" {
                return if child.text().contains("pub") {
                    Some(Visibility::Public)
                } else {
                    Some(Visibility::Private)
                };
            }
        }
        Some(Visibility::Private)
    }

    fn get_receiver_type(&self, node: &SyntaxNode, source: &str) -> Option<String> {
        // Rust 方法本身不嵌在 struct/trait 节点下，需向上找到 impl_item 推断 receiver 类型。
        let mut parent = node.parent();
        while let Some(p) = parent {
            if p.type_name() == "impl_item" {
                let type_idents = p
                    .named_children()
                    .into_iter()
                    .filter(|c| c.type_name() == "type_identifier")
                    .collect::<Vec<_>>();
                if let Some(type_node) = type_idents.last() {
                    return Some(get_node_text(type_node, source));
                }
                if let Some(generic_type) = p
                    .named_children()
                    .into_iter()
                    .find(|c| c.type_name() == "generic_type")
                    && let Some(inner_type) = generic_type
                        .named_children()
                        .into_iter()
                        .find(|c| c.type_name() == "type_identifier")
                {
                    return Some(get_node_text(&inner_type, source));
                }
                return None;
            }
            parent = p.parent();
        }
        None
    }

    fn extract_import(&self, node: &SyntaxNode, source: &str) -> Option<ImportInfo> {
        let import_text = get_node_text(node, source).trim().to_string();
        let use_arg = node.named_children().into_iter().find(|c| {
            matches!(
                c.type_name().as_str(),
                "scoped_use_list" | "scoped_identifier" | "use_list" | "identifier"
            )
        })?;
        Some(ImportInfo {
            module_name: root_module(&use_arg, source),
            signature: import_text,
            handled_refs: false,
        })
    }
}
