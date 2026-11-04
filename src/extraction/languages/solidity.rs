//! Solidity declaration nodes from the pinned native grammar.
use crate::extraction::tree_sitter_helpers::get_node_text;
use crate::extraction::tree_sitter_types::{ExtractorContext, LanguageExtractor, NodeExtra};
use crate::types::NodeKind;
use crate::web_tree_sitter::SyntaxNode;

pub struct SolidityExtractor;
pub const SOLIDITY_EXTRACTOR: SolidityExtractor = SolidityExtractor;
impl LanguageExtractor for SolidityExtractor {
    fn visit_node(&self, node: &SyntaxNode, ctx: &mut dyn ExtractorContext) -> bool {
        let kind = match node.node_type() {
            "contract_declaration" | "library_declaration" => NodeKind::Class,
            "interface_declaration" => NodeKind::Interface,
            "struct_declaration" => NodeKind::Struct,
            "event_definition" => NodeKind::Field,
            "modifier_definition" => NodeKind::Method,
            _ => return false,
        };
        let Some(name_node) = node.child_for_field_name("name") else {
            return true;
        };
        let name = get_node_text(name_node, ctx.source());
        let extra = NodeExtra {
            signature: (node.node_type() == "event_definition").then(|| {
                get_node_text(node, ctx.source())
                    .trim()
                    .chars()
                    .take(200)
                    .collect()
            }),
            ..NodeExtra::default()
        };
        let Some(created) = ctx.create_node(kind, &name, node, extra) else {
            return true;
        };
        // Only containers are traversed here. Event parameters, struct members
        // and modifier locals are outside this declaration-only scope.
        if matches!(kind, NodeKind::Class | NodeKind::Interface) {
            ctx.push_scope(created.id);
            if let Some(body) = node.child_for_field_name("body") {
                for child in &body.named_children {
                    ctx.visit_node(child);
                }
            }
            ctx.pop_scope();
        }
        true
    }

    fn function_types(&self) -> &[&'static str] {
        &[]
    }
    fn class_types(&self) -> &[&'static str] {
        &[]
    }
    fn method_types(&self) -> &[&'static str] {
        &[]
    }
    fn interface_types(&self) -> &[&'static str] {
        &[]
    }
    fn struct_types(&self) -> &[&'static str] {
        &[]
    }
    fn enum_types(&self) -> &[&'static str] {
        &[]
    }
    fn type_alias_types(&self) -> &[&'static str] {
        &[]
    }
    fn import_types(&self) -> &[&'static str] {
        &[]
    }
    fn call_types(&self) -> &[&'static str] {
        &[]
    }
    fn variable_types(&self) -> &[&'static str] {
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
}
