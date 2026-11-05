//! VB.NET declaration and reference extraction using the pinned native grammar.
use crate::extraction::tree_sitter_types::{
    ExtractorContext, ImportInfo, LanguageExtractor, NodeExtra, UnresolvedReferenceInput,
};
use crate::types::{NodeKind, ReferenceKind, Visibility};
use crate::web_tree_sitter::SyntaxNode;

fn has_modifier(node: &SyntaxNode, modifier: &str) -> bool {
    node.named_children.iter().any(|child| {
        child.node_type() == "member_modifier" && child.text.eq_ignore_ascii_case(modifier)
    })
}
pub struct VbnetExtractor;
pub const VBNET_EXTRACTOR: VbnetExtractor = VbnetExtractor;
impl LanguageExtractor for VbnetExtractor {
    fn visit_node(&self, node: &SyntaxNode, ctx: &mut dyn ExtractorContext) -> bool {
        if matches!(
            node.node_type(),
            "inherits_statement" | "implements_statement"
        ) {
            if let Some(owner) = ctx.node_stack().last().cloned() {
                for target in &node.named_children {
                    ctx.add_unresolved_reference(UnresolvedReferenceInput {
                        from_node_id: owner.clone(),
                        reference_name: target.text.clone(),
                        reference_kind: if node.node_type() == "inherits_statement" {
                            ReferenceKind::Extends
                        } else {
                            ReferenceKind::Implements
                        },
                        line: Some(target.start_position.row + 1),
                        column: Some(target.start_position.column),
                        file_path: None,
                    });
                }
            }
            return true;
        }
        let kind = match node.node_type() {
            "namespace_declaration" => NodeKind::Namespace,
            "property_declaration"
            | "interface_property_declaration"
            | "abstract_property_declaration" => NodeKind::Property,
            "event_declaration" | "custom_event_declaration" | "interface_event_declaration" => {
                NodeKind::Field
            }
            _ => return false,
        };
        let name = node.child_for_field_name("name").map(|n| n.text.as_str());
        if let Some(name) = name
            && let Some(created) = ctx.create_node(kind, name, node, NodeExtra::default())
        {
            if kind == NodeKind::Namespace {
                ctx.push_scope(created.id);
                for child in &node.named_children {
                    ctx.visit_node(child);
                }
                ctx.pop_scope();
            } else if kind == NodeKind::Property {
                ctx.push_scope(created.id.clone());
                ctx.visit_function_body(node, &created.id);
                ctx.pop_scope();
            }
        }
        true
    }
    fn property_types(&self) -> &[&'static str] {
        &[
            "property_declaration",
            "interface_property_declaration",
            "abstract_property_declaration",
        ]
    }
    fn resolve_body(&self, node: &SyntaxNode, _body_field: &str) -> Option<SyntaxNode> {
        // The grammar repeats the body field on each member instead of wrapping it.
        Some(node.clone())
    }
    fn get_visibility(&self, node: &SyntaxNode) -> Option<Visibility> {
        Some(if has_modifier(node, "private") {
            Visibility::Private
        } else if has_modifier(node, "protected") {
            Visibility::Protected
        } else if has_modifier(node, "friend") {
            Visibility::Internal
        } else {
            Visibility::Public
        })
    }
    fn is_exported(&self, node: &SyntaxNode, _source: &str) -> bool {
        self.get_visibility(node) == Some(Visibility::Public)
    }
    fn pre_parse(&self, source: &str) -> Option<String> {
        // Preserve all existing offsets while fixing the grammar's missing EOF terminator.
        (!source.ends_with('\n')).then(|| format!("{source}\n"))
    }
    fn function_types(&self) -> &[&'static str] {
        &[]
    }
    fn class_types(&self) -> &[&'static str] {
        &["class_declaration", "module_declaration"]
    }
    fn method_types(&self) -> &[&'static str] {
        &["method_declaration"]
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
    fn extract_import(&self, node: &SyntaxNode, _source: &str) -> Option<ImportInfo> {
        Some(ImportInfo {
            module_name: node.child_for_field_name("namespace")?.text.clone(),
            signature: node.text.trim().to_owned(),
            handled_refs: false,
        })
    }
    fn import_types(&self) -> &[&'static str] {
        &["imports_statement"]
    }
    fn call_types(&self) -> &[&'static str] {
        &[
            "invocation_expression",
            "array_access_expression",
            "generic_invocation_expression",
        ]
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
