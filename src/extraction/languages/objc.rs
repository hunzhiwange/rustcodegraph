//! Rust translation of `src/extraction/languages/objc.ts`.

use crate::extraction::tree_sitter_helpers::{get_child_by_field, get_node_text};
use crate::extraction::tree_sitter_types::{
    ExtractorContext, ImportInfo, LanguageExtractor, NodeExtra,
};
use crate::types::NodeKind;
use crate::web_tree_sitter::SyntaxNode;

pub struct ObjcExtractor;

pub const OBJC_EXTRACTOR: ObjcExtractor = ObjcExtractor;

pub fn objc_extractor() -> &'static ObjcExtractor {
    &OBJC_EXTRACTOR
}

fn find_compound_statement(node: &SyntaxNode) -> Option<SyntaxNode> {
    node.named_children()
        .into_iter()
        .find(|child| child.type_name() == "compound_statement")
}

fn extract_objc_method_name(node: &SyntaxNode, source: &str) -> Option<String> {
    if node.type_name() != "method_definition" && node.type_name() != "method_declaration" {
        return None;
    }
    let identifiers = node
        .named_children()
        .into_iter()
        .filter(|c| c.type_name() == "identifier")
        .collect::<Vec<_>>();
    let first = identifiers.first()?;
    let has_parameters = node
        .named_children()
        .into_iter()
        .any(|c| c.type_name() == "method_parameter");
    if !has_parameters {
        return Some(get_node_text(first, source));
    }
    // Objective-C selector 的完整方法名由每段参数标签拼接成 `foo:bar:`。
    Some(
        identifiers
            .iter()
            .map(|id| format!("{}:", get_node_text(id, source)))
            .collect::<String>(),
    )
}

const OBJC_TYPE_QUALIFIERS: &[&str] = &[
    "nonnull",
    "nullable",
    "null_unspecified",
    "null_resettable",
    "_Nonnull",
    "_Nullable",
    "_Null_unspecified",
    "__nonnull",
    "__nullable",
    "const",
    "volatile",
    "strong",
    "weak",
    "copy",
    "assign",
    "retain",
    "oneway",
    "__strong",
    "__weak",
    "__unsafe_unretained",
    "__autoreleasing",
    "__kindof",
];

fn collect_type_identifiers(node: &SyntaxNode, source: &str, out: &mut Vec<String>) {
    if node.type_name() == "type_identifier" {
        out.push(get_node_text(node, source).trim().to_string());
    }
    for child in node.named_children() {
        collect_type_identifiers(&child, source, out);
    }
}

fn extract_objc_return_type(node: &SyntaxNode, source: &str) -> Option<String> {
    if node.type_name() != "method_definition" && node.type_name() != "method_declaration" {
        return None;
    }
    let method_type = node
        .named_children()
        .into_iter()
        .find(|c| c.type_name() == "method_type")?;
    let mut ids = Vec::new();
    collect_type_identifiers(&method_type, source, &mut ids);
    // nullable/ownership 等修饰符会和真实类型同级出现，只保留第一个业务类型标识符。
    let name = ids
        .into_iter()
        .find(|name| !OBJC_TYPE_QUALIFIERS.contains(&name.as_str()))?;
    if !is_identifier(&name) || matches!(name.as_str(), "void" | "id" | "instancetype") {
        return None;
    }
    Some(name)
}

fn extract_objc_property_name(node: &SyntaxNode, source: &str) -> Option<String> {
    if node.type_name() != "property_declaration" {
        return None;
    }
    let struct_decl = node
        .named_children()
        .into_iter()
        .find(|c| c.type_name() == "struct_declaration")?;
    let struct_declarator = struct_decl
        .named_children()
        .into_iter()
        .find(|c| c.type_name() == "struct_declarator")?;
    let mut current = Some(struct_declarator);
    // property 声明可能包着 pointer_declarator，多层向内剥直到真正 identifier。
    while let Some(c) = current {
        let inner = get_child_by_field(&c, "declarator").cloned().or_else(|| {
            c.named_children().into_iter().find(|child| {
                child.type_name() == "identifier" || child.type_name() == "pointer_declarator"
            })
        });
        let Some(inner) = inner else {
            break;
        };
        if inner.type_name() == "identifier" {
            return Some(get_node_text(&inner, source));
        }
        current = Some(inner);
    }
    let text = get_node_text(node, source);
    text.trim_end_matches(';')
        .split(|ch: char| !(ch == '_' || ch.is_ascii_alphanumeric()))
        .rfind(|part| !part.is_empty())
        .map(str::to_owned)
}

fn is_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some('_') | Some('A'..='Z') | Some('a'..='z'))
        && chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
}

fn extract_objc_include(node: &SyntaxNode, source: &str) -> Option<ImportInfo> {
    let import_text = get_node_text(node, source).trim().to_string();
    if let Some(system_lib) = node
        .named_children()
        .into_iter()
        .find(|c| c.type_name() == "system_lib_string")
    {
        return Some(ImportInfo {
            module_name: get_node_text(&system_lib, source)
                .trim_matches(['<', '>'])
                .to_string(),
            signature: import_text,
            handled_refs: false,
        });
    }
    let string_literal = node
        .named_children()
        .into_iter()
        .find(|c| c.type_name() == "string_literal")?;
    let string_content = string_literal
        .named_children()
        .into_iter()
        .find(|c| c.type_name() == "string_content")?;
    Some(ImportInfo {
        module_name: get_node_text(&string_content, source),
        signature: import_text,
        handled_refs: false,
    })
}

impl LanguageExtractor for ObjcExtractor {
    fn function_types(&self) -> &'static [&'static str] {
        &["function_definition"]
    }

    fn class_types(&self) -> &'static [&'static str] {
        &["class_interface"]
    }

    fn method_types(&self) -> &'static [&'static str] {
        &["method_definition"]
    }

    fn interface_types(&self) -> &'static [&'static str] {
        &["protocol_declaration"]
    }

    fn interface_kind(&self) -> Option<NodeKind> {
        Some(NodeKind::Protocol)
    }

    fn struct_types(&self) -> &'static [&'static str] {
        &["struct_specifier"]
    }

    fn enum_types(&self) -> &'static [&'static str] {
        &["enum_specifier"]
    }

    fn enum_member_types(&self) -> &'static [&'static str] {
        &["enumerator"]
    }

    fn type_alias_types(&self) -> &'static [&'static str] {
        &["type_definition"]
    }

    fn import_types(&self) -> &'static [&'static str] {
        &["preproc_include"]
    }

    fn call_types(&self) -> &'static [&'static str] {
        &["call_expression", "message_expression"]
    }

    fn variable_types(&self) -> &'static [&'static str] {
        &["declaration"]
    }

    fn property_types(&self) -> &'static [&'static str] {
        &["property_declaration"]
    }

    fn name_field(&self) -> &'static str {
        "declarator"
    }

    fn body_field(&self) -> &'static str {
        "body"
    }

    fn params_field(&self) -> &'static str {
        "parameters"
    }

    fn get_return_type(&self, node: &SyntaxNode, source: &str) -> Option<String> {
        extract_objc_return_type(node, source)
    }

    fn resolve_name(&self, node: &SyntaxNode, source: &str) -> Option<String> {
        extract_objc_method_name(node, source)
    }

    fn extract_property_name(&self, node: &SyntaxNode, source: &str) -> Option<String> {
        extract_objc_property_name(node, source)
    }

    fn resolve_body(&self, node: &SyntaxNode, body_field: &str) -> Option<SyntaxNode> {
        get_child_by_field(node, body_field)
            .cloned()
            .or_else(|| find_compound_statement(node))
    }

    fn resolve_type_alias_kind(&self, node: &SyntaxNode, _source: &str) -> Option<NodeKind> {
        // Objective-C 复用 C 的 typedef 模式，内联 struct/enum 要提升成对应 kind。
        for child in node.named_children() {
            if child.type_name() == "enum_specifier" && get_child_by_field(&child, "body").is_some()
            {
                return Some(NodeKind::Enum);
            }
            if child.type_name() == "struct_specifier"
                && get_child_by_field(&child, "body").is_some()
            {
                return Some(NodeKind::Struct);
            }
        }
        None
    }

    fn is_static(&self, node: &SyntaxNode) -> bool {
        node.text().trim_start().starts_with('+')
    }

    fn visit_node(&self, node: &SyntaxNode, ctx: &mut dyn ExtractorContext) -> bool {
        if node.type_name() != "class_implementation" {
            return false;
        }
        // `@implementation` 里的方法应挂到同名 `@interface` 类节点下；没有 interface 时才补建类。
        let Some(class_name_node) = node
            .named_children()
            .into_iter()
            .find(|c| c.type_name() == "identifier")
        else {
            return true;
        };
        let class_name = get_node_text(&class_name_node, ctx.source());
        let class_node = ctx
            .nodes()
            .iter()
            .find(|n| {
                n.name == class_name && n.file_path == ctx.file_path() && n.kind == NodeKind::Class
            })
            .cloned()
            .or_else(|| ctx.create_node(NodeKind::Class, &class_name, node, NodeExtra::default()));
        let Some(class_node) = class_node else {
            return true;
        };
        ctx.push_scope(class_node.id.clone());
        for child in node.named_children() {
            if child.type_name() == "implementation_definition" {
                for impl_child in child.named_children() {
                    ctx.visit_node(&impl_child);
                }
            }
        }
        ctx.pop_scope();
        true
    }

    fn extract_import(&self, node: &SyntaxNode, source: &str) -> Option<ImportInfo> {
        extract_objc_include(node, source)
    }
}
