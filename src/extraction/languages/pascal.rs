//! Rust translation of `src/extraction/languages/pascal.ts`.

use crate::extraction::tree_sitter_helpers::{get_child_by_field, get_node_text};
use crate::extraction::tree_sitter_types::{LanguageExtractor, Visibility};
use crate::web_tree_sitter::SyntaxNode;

pub struct PascalExtractor;

pub const PASCAL_EXTRACTOR: PascalExtractor = PascalExtractor;

pub fn pascal_extractor() -> &'static PascalExtractor {
    &PASCAL_EXTRACTOR
}

impl LanguageExtractor for PascalExtractor {
    fn function_types(&self) -> &'static [&'static str] {
        &["declProc"]
    }

    fn class_types(&self) -> &'static [&'static str] {
        &["declClass"]
    }

    fn method_types(&self) -> &'static [&'static str] {
        &["declProc"]
    }

    fn interface_types(&self) -> &'static [&'static str] {
        &["declIntf"]
    }

    fn struct_types(&self) -> &'static [&'static str] {
        &[]
    }

    fn enum_types(&self) -> &'static [&'static str] {
        &["declEnum"]
    }

    fn type_alias_types(&self) -> &'static [&'static str] {
        &["declType"]
    }

    fn import_types(&self) -> &'static [&'static str] {
        &["declUses"]
    }

    fn call_types(&self) -> &'static [&'static str] {
        &["exprCall"]
    }

    fn variable_types(&self) -> &'static [&'static str] {
        &["declField", "declConst"]
    }

    fn name_field(&self) -> &'static str {
        "name"
    }

    fn body_field(&self) -> &'static str {
        "body"
    }

    fn params_field(&self) -> &'static str {
        "args"
    }

    fn return_field(&self) -> Option<&'static str> {
        Some("type")
    }

    fn get_return_type(&self, node: &SyntaxNode, source: &str) -> Option<String> {
        // Pascal 的函数返回类型以 typeref 出现，过程没有该节点；只抽可作为接收者的标识符。
        let typeref = node
            .named_children()
            .into_iter()
            .find(|c| c.type_name() == "typeref")?;
        let id = typeref
            .named_children()
            .into_iter()
            .find(|c| c.type_name() == "identifier")
            .unwrap_or(typeref);
        let name = get_node_text(&id, source).trim().to_string();
        is_identifier(&name).then_some(name)
    }

    fn get_signature(&self, node: &SyntaxNode, source: &str) -> Option<String> {
        let args = get_child_by_field(node, "args");
        let return_type = node
            .named_children()
            .into_iter()
            .find(|c| c.type_name() == "typeref");
        if args.is_none() && return_type.is_none() {
            return None;
        }
        let mut sig = args
            .as_ref()
            .map(|args| get_node_text(args, source))
            .unwrap_or_default();
        if let Some(return_type) = return_type {
            sig.push_str(": ");
            sig.push_str(&get_node_text(&return_type, source));
        }
        (!sig.is_empty()).then_some(sig)
    }

    fn get_visibility(&self, node: &SyntaxNode) -> Option<Visibility> {
        // 可见性在 Delphi/Pascal 的 section 上，而不是每个成员自身；向父级扫描最近的 declSection。
        let mut current = node.parent();
        while let Some(parent) = current {
            if parent.type_name() == "declSection" {
                for i in 0..parent.child_count() {
                    let Some(child) = parent.child(i) else {
                        continue;
                    };
                    if child.type_name() == "kPublic" || child.type_name() == "kPublished" {
                        return Some(Visibility::Public);
                    }
                    if child.type_name() == "kPrivate" {
                        return Some(Visibility::Private);
                    }
                    if child.type_name() == "kProtected" {
                        return Some(Visibility::Protected);
                    }
                }
            }
            current = parent.parent();
        }
        None
    }

    fn is_exported(&self, _node: &SyntaxNode, _source: &str) -> bool {
        false
    }

    fn is_static(&self, node: &SyntaxNode) -> bool {
        // `class function/procedure` 表示静态成员。
        (0..node.child_count()).any(|i| node.child(i).is_some_and(|c| c.type_name() == "kClass"))
    }

    fn is_const(&self, node: &SyntaxNode) -> bool {
        node.type_name() == "declConst"
    }
}

fn is_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some('_') | Some('A'..='Z') | Some('a'..='z'))
        && chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
}
