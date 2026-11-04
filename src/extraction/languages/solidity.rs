//! Solidity declarations and syntactically evidenced unresolved references.
use crate::extraction::tree_sitter_helpers::get_node_text;
use crate::extraction::tree_sitter_types::{
    ExtractorContext, LanguageExtractor, NodeExtra, UnresolvedReferenceInput,
};
use crate::types::{NodeKind, ReferenceKind};
use crate::web_tree_sitter::SyntaxNode;

pub struct SolidityExtractor;
pub const SOLIDITY_EXTRACTOR: SolidityExtractor = SolidityExtractor;
impl LanguageExtractor for SolidityExtractor {
    fn visit_node(&self, node: &SyntaxNode, ctx: &mut dyn ExtractorContext) -> bool {
        let t = node.node_type();
        if matches!(
            t,
            "call_expression" | "modifier_invocation" | "new_expression"
        ) {
            let target = node
                .child_for_field_name("function")
                .or_else(|| node.child_for_field_name("name"))
                .or_else(|| node.child_for_field_name("type"))
                .or_else(|| node.named_child(0));
            if let (Some(owner), Some(target)) = (ctx.node_stack().last().cloned(), target) {
                // A new-expression nested in a call owns the constructor reference.
                if target.node_type() != "new_expression" {
                    let name = get_node_text(target, ctx.source());
                    add_ref(
                        ctx,
                        owner,
                        name,
                        if t == "new_expression" {
                            ReferenceKind::Instantiates
                        } else {
                            ReferenceKind::Calls
                        },
                        node,
                    );
                }
            }
            for child in &node.named_children {
                ctx.visit_node(child);
            }
            return true;
        }
        if matches!(
            t,
            "function_definition" | "constructor_definition" | "fallback_receive_definition"
        ) {
            let name = node
                .child_for_field_name("name")
                .map(|n| get_node_text(n, ctx.source()))
                .unwrap_or_else(|| {
                    if t == "constructor_definition" {
                        "constructor".into()
                    } else if get_node_text(node, ctx.source()).starts_with("receive") {
                        "receive".into()
                    } else {
                        "fallback".into()
                    }
                });
            let kind = if ctx.nodes().iter().any(|n| {
                ctx.node_stack().last() == Some(&n.id)
                    && matches!(n.kind, NodeKind::Class | NodeKind::Interface)
            }) {
                NodeKind::Method
            } else {
                NodeKind::Function
            };
            if let Some(created) = ctx.create_node(kind, &name, node, NodeExtra::default()) {
                ctx.push_scope(created.id);
                for child in &node.named_children {
                    ctx.visit_node(child);
                }
                ctx.pop_scope();
            }
            return true;
        }
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
        if matches!(kind, NodeKind::Class | NodeKind::Interface) {
            for child in &node.named_children {
                if child.node_type() == "inheritance_specifier" {
                    if let Some(ancestor) = child.child_for_field_name("ancestor") {
                        let name = get_node_text(ancestor, ctx.source());
                        add_ref(
                            ctx,
                            created.id.clone(),
                            name,
                            ReferenceKind::Extends,
                            ancestor,
                        );
                    }
                }
            }
        }
        // Traverse callable bodies under their own scope; event parameters and
        // struct members are outside this extraction scope.
        if matches!(
            kind,
            NodeKind::Class | NodeKind::Interface | NodeKind::Method
        ) {
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

fn add_ref(
    ctx: &mut dyn ExtractorContext,
    owner: String,
    name: String,
    kind: ReferenceKind,
    node: &SyntaxNode,
) {
    ctx.add_unresolved_reference(UnresolvedReferenceInput {
        from_node_id: owner,
        reference_name: name,
        reference_kind: kind,
        line: Some(node.start_position.row + 1),
        column: Some(node.start_position.column),
        file_path: None,
    });
}
