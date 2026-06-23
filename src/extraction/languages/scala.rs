//! Rust translation of `src/extraction/languages/scala.ts`.

use crate::extraction::tree_sitter_helpers::get_node_text;
use crate::extraction::tree_sitter_types::{
    ClassClassification, ExtractorContext, ImportInfo, LanguageExtractor, NodeExtra,
    UnresolvedReferenceInput, Visibility,
};
use crate::types::{NodeKind, ReferenceKind};
use crate::web_tree_sitter::SyntaxNode;

pub struct ScalaExtractor;

pub const SCALA_EXTRACTOR: ScalaExtractor = ScalaExtractor;

pub fn scala_extractor() -> &'static ScalaExtractor {
    &SCALA_EXTRACTOR
}

fn get_val_var_name(node: &SyntaxNode, source: &str) -> Option<String> {
    // val/var 的 pattern 可能是简单 identifier，也可能被模式节点包住。
    let pattern_node = node.child_for_field_name("pattern")?;
    if pattern_node.type_name() == "identifier" {
        return Some(get_node_text(pattern_node, source));
    }
    pattern_node
        .named_children()
        .into_iter()
        .find(|c| c.type_name() == "identifier")
        .map(|ident| get_node_text(&ident, source))
}

const SCALA_BUILTIN_TYPES: &[&str] = &[
    "Int", "Long", "Short", "Byte", "Float", "Double", "Boolean", "Char", "Unit", "String", "Any",
    "AnyRef", "AnyVal", "Nothing", "Null",
];

fn emit_scala_type_refs(type_node: &SyntaxNode, from_id: &str, ctx: &mut dyn ExtractorContext) {
    if type_node.type_name() == "type_identifier" {
        let name = get_node_text(type_node, ctx.source());
        if !name.is_empty() && !SCALA_BUILTIN_TYPES.contains(&name.as_str()) {
            // 字段/变量的显式类型对影响分析很有价值，跳过内置类型以减少无意义引用。
            ctx.add_unresolved_reference(UnresolvedReferenceInput {
                from_node_id: from_id.to_string(),
                reference_name: name,
                reference_kind: ReferenceKind::References,
                file_path: None,
                line: Some(type_node.start_position().row + 1),
                column: Some(type_node.start_position().column),
            });
        }
        return;
    }
    for child in type_node.named_children() {
        emit_scala_type_refs(&child, from_id, ctx);
    }
}

fn extract_scala_return_type(node: &SyntaxNode, source: &str) -> Option<String> {
    let rt = node.child_for_field_name("return_type")?;
    let raw = get_node_text(rt, source).trim().to_string();
    if raw.starts_with("this.") {
        return None;
    }
    let base = strip_square_generics(&raw).replace(char::is_whitespace, "");
    let last = base.split('.').next_back()?;
    is_identifier(last).then(|| last.to_string())
}

fn strip_square_generics(input: &str) -> String {
    let mut out = String::new();
    let mut depth = 0usize;
    for ch in input.chars() {
        match ch {
            '[' => depth += 1,
            ']' => depth = depth.saturating_sub(1),
            _ if depth == 0 => out.push(ch),
            _ => {}
        }
    }
    out
}

fn extract_visibility(node: &SyntaxNode) -> Visibility {
    for child in node.named_children() {
        if child.type_name() == "modifiers" || child.type_name() == "access_modifier" {
            let text = child.text();
            if text.contains("private") {
                return Visibility::Private;
            }
            if text.contains("protected") {
                return Visibility::Protected;
            }
        }
    }
    Visibility::Public
}

fn is_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some('_') | Some('A'..='Z') | Some('a'..='z'))
        && chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
}

impl LanguageExtractor for ScalaExtractor {
    fn function_types(&self) -> &'static [&'static str] {
        &[]
    }

    fn class_types(&self) -> &'static [&'static str] {
        &["class_definition", "object_definition", "trait_definition"]
    }

    fn method_types(&self) -> &'static [&'static str] {
        &["function_definition", "function_declaration"]
    }

    fn interface_types(&self) -> &'static [&'static str] {
        &[]
    }

    fn struct_types(&self) -> &'static [&'static str] {
        &[]
    }

    fn enum_types(&self) -> &'static [&'static str] {
        &["enum_definition"]
    }

    fn enum_member_types(&self) -> &'static [&'static str] {
        &["simple_enum_case", "full_enum_case", "enumerator"]
    }

    fn type_alias_types(&self) -> &'static [&'static str] {
        &["type_definition"]
    }

    fn import_types(&self) -> &'static [&'static str] {
        &["import_declaration"]
    }

    fn call_types(&self) -> &'static [&'static str] {
        &["call_expression"]
    }

    fn variable_types(&self) -> &'static [&'static str] {
        &[]
    }

    fn field_types(&self) -> &'static [&'static str] {
        &[]
    }

    fn extra_class_node_types(&self) -> &'static [&'static str] {
        &[]
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
        extract_scala_return_type(node, source)
    }

    fn interface_kind(&self) -> Option<NodeKind> {
        Some(NodeKind::Trait)
    }

    fn classify_class_node(&self, node: &SyntaxNode) -> ClassClassification {
        if node.type_name() == "trait_definition" {
            ClassClassification::Trait
        } else {
            ClassClassification::Class
        }
    }

    fn get_signature(&self, node: &SyntaxNode, source: &str) -> Option<String> {
        let params = node.child_for_field_name("parameters");
        let return_type = node.child_for_field_name("return_type");
        if params.is_none() && return_type.is_none() {
            return None;
        }
        let mut sig = params
            .as_ref()
            .map(|params| get_node_text(params, source))
            .unwrap_or_default();
        if let Some(return_type) = return_type {
            sig.push_str(": ");
            sig.push_str(&get_node_text(return_type, source));
        }
        (!sig.is_empty()).then_some(sig)
    }

    fn get_visibility(&self, node: &SyntaxNode) -> Option<Visibility> {
        Some(extract_visibility(node))
    }

    fn is_async(&self, _node: &SyntaxNode) -> bool {
        false
    }

    fn is_static(&self, node: &SyntaxNode) -> bool {
        node.named_children()
            .into_iter()
            .any(|c| c.type_name() == "modifiers" && c.text().contains("static"))
    }

    fn visit_node(&self, node: &SyntaxNode, ctx: &mut dyn ExtractorContext) -> bool {
        let t = node.type_name();
        if t == "val_definition" || t == "var_definition" {
            return visit_scala_val_var(node, ctx);
        }

        if t == "enum_case_definitions" {
            // Scala enum case 可以批量出现在 enum_case_definitions 下，通用 enum member 遍历覆盖不到。
            for child in node.named_children() {
                if (child.type_name() == "simple_enum_case"
                    || child.type_name() == "full_enum_case")
                    && let Some(name_node) = child.child_for_field_name("name")
                {
                    ctx.create_node(
                        NodeKind::EnumMember,
                        &get_node_text(name_node, ctx.source()),
                        &child,
                        NodeExtra::default(),
                    );
                }
            }
            return true;
        }

        if t == "extension_definition" {
            // extension 自身不是图节点，但内部方法需要在当前 scope 下继续访问。
            if let Some(body) = node.child_for_field_name("body") {
                for child in body.named_children() {
                    ctx.visit_node(&child);
                }
            }
            return true;
        }

        false
    }

    fn extract_import(&self, node: &SyntaxNode, source: &str) -> Option<ImportInfo> {
        let import_text = get_node_text(node, source).trim().to_string();
        if let Some(path_node) = node.child_for_field_name("path") {
            return Some(ImportInfo {
                module_name: get_node_text(path_node, source),
                signature: import_text,
                handled_refs: false,
            });
        }
        let child = node
            .named_children()
            .into_iter()
            .find(|c| c.type_name() == "identifier" || c.type_name() == "stable_identifier")?;
        Some(ImportInfo {
            module_name: get_node_text(&child, source),
            signature: import_text,
            handled_refs: false,
        })
    }
}

fn visit_scala_val_var(node: &SyntaxNode, ctx: &mut dyn ExtractorContext) -> bool {
    let Some(name) = get_val_var_name(node, ctx.source()) else {
        return false;
    };
    let is_instance_field = ctx.node_stack().last().is_some_and(|owner_id| {
        // 在 class/trait/enum 作用域下的 val/var 是 field，其余顶层绑定才是变量/常量。
        ctx.nodes().iter().any(|node| {
            &node.id == owner_id
                && matches!(
                    node.kind,
                    NodeKind::Class | NodeKind::Trait | NodeKind::Enum | NodeKind::Struct
                )
                && !node.name.is_empty()
        })
    });
    let kind = if is_instance_field {
        NodeKind::Field
    } else if node.type_name() == "val_definition" {
        NodeKind::Constant
    } else {
        NodeKind::Variable
    };
    let type_node = node.child_for_field_name("type");
    let extra = NodeExtra {
        visibility: Some(extract_visibility(node)),
        signature: type_node.as_ref().map(|ty| {
            format!(
                "{} {}: {}",
                if node.type_name() == "val_definition" {
                    "val"
                } else {
                    "var"
                },
                name,
                get_node_text(ty, ctx.source())
            )
        }),
        ..Default::default()
    };
    let created = ctx.create_node(kind, &name, node, extra);
    if let (Some(created), Some(type_node)) = (created, type_node) {
        emit_scala_type_refs(type_node, &created.id, ctx);
    }
    true
}
