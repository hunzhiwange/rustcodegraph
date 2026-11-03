//! Nix declarations, extracted from the pinned native grammar.
use crate::extraction::tree_sitter_helpers::get_node_text;
use crate::extraction::tree_sitter_types::{ExtractorContext, LanguageExtractor, NodeExtra};
use crate::types::NodeKind;
use crate::web_tree_sitter::SyntaxNode;

pub struct NixExtractor;
pub const NIX_EXTRACTOR: NixExtractor = NixExtractor;
pub fn nix_extractor() -> &'static NixExtractor {
    &NIX_EXTRACTOR
}

impl LanguageExtractor for NixExtractor {
    fn visit_node(&self, node: &SyntaxNode, ctx: &mut dyn ExtractorContext) -> bool {
        if node.node_type() != "source_code" {
            return false;
        }
        walk_declarations(node, ctx, true, false, false, false);
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
        "formals"
    }
}

// Ancestor links in the native AST are childless stubs. Carry return-position
// information down the real tree instead of looking up a let body's field on
// an ancestor (which would misclassify every returned attrset as private).
fn walk_declarations(
    node: &SyntaxNode,
    ctx: &mut dyn ExtractorContext,
    returned: bool,
    member: bool,
    in_function: bool,
    local: bool,
) {
    match node.node_type() {
        "binding" => {
            let Some(path) = node.child_for_field_name("attrpath") else {
                return;
            };
            if dynamic_attribute(path) {
                return;
            }
            let name = get_node_text(path, ctx.source()).trim().to_owned();
            let Some(value) = node.child_for_field_name("expression") else {
                return;
            };
            if name.is_empty() {
                return;
            }
            if value.node_type() == "function_expression" {
                let (signature, body) = lambda_parts(value, ctx.source());
                if let Some(created) = ctx.create_node(
                    NodeKind::Function,
                    &name,
                    node,
                    NodeExtra {
                        signature: Some(signature),
                        is_exported: Some(member),
                        ..Default::default()
                    },
                ) {
                    ctx.push_scope(created.id);
                    if let Some(body) = body {
                        walk_declarations(body, ctx, false, false, true, false);
                    }
                    ctx.pop_scope();
                }
            } else {
                if !local {
                    let text = get_node_text(value, ctx.source());
                    let preview: String = text.chars().take(100).collect();
                    ctx.create_node(
                        NodeKind::Variable,
                        &name,
                        node,
                        NodeExtra {
                            signature: Some(format!(
                                "= {preview}{}",
                                if text.chars().count() > 100 {
                                    "..."
                                } else {
                                    ""
                                }
                            )),
                            is_exported: Some(member),
                            ..Default::default()
                        },
                    );
                }
                walk_declarations(value, ctx, false, false, in_function, false);
            }
        }
        "let_expression" => {
            let body = node.child_for_field_name("body");
            for child in &node.named_children {
                if body.is_some_and(|body| body.id == child.id) {
                    walk_declarations(child, ctx, returned, false, in_function, false);
                } else {
                    // Function-local scalar bindings are implementation details;
                    // keep named functions and returned API attributes searchable.
                    walk_declarations(child, ctx, false, false, in_function, in_function);
                }
            }
        }
        "attrset_expression" | "rec_attrset_expression" => {
            for child in &node.named_children {
                walk_declarations(child, ctx, returned, returned, in_function, false);
            }
        }
        "function_expression" => {
            // Anonymous/curried lambdas have no independent symbol or parameters.
            if let Some(body) = node.child_for_field_name("body") {
                walk_declarations(body, ctx, returned, false, true, false);
            }
        }
        "inherit" | "inherit_from" => {
            if !local && let Some(attrs) = node.child_for_field_name("attrs") {
                for attr in &attrs.named_children {
                    if dynamic_attribute(attr) {
                        continue;
                    }
                    let name = get_node_text(attr, ctx.source());
                    ctx.create_node(
                        NodeKind::Variable,
                        &name,
                        attr,
                        NodeExtra {
                            is_exported: Some(member),
                            ..Default::default()
                        },
                    );
                }
            }
        }
        "formals" | "formal" => {}
        _ => {
            for child in &node.named_children {
                walk_declarations(child, ctx, returned, member, in_function, local);
            }
        }
    }
}

fn dynamic_attribute(node: &SyntaxNode) -> bool {
    matches!(node.node_type(), "interpolation") || node.named_children.iter().any(dynamic_attribute)
}

fn lambda_parts<'a>(node: &'a SyntaxNode, source: &str) -> (String, Option<&'a SyntaxNode>) {
    let mut current = node;
    let mut params = Vec::new();
    while current.node_type() == "function_expression" {
        let Some(body) = current.child_for_field_name("body") else {
            return ("()".to_owned(), None);
        };
        let prefix = source
            .get(current.start_index..body.start_index)
            .unwrap_or("")
            .trim();
        params.push(prefix.trim_end_matches(':').trim().to_owned());
        current = body;
    }
    let signature = match params.as_slice() {
        [] => "()".to_owned(),
        [param] if !param.contains(['{', '@']) && !param.starts_with('(') => format!("({param})"),
        _ => params.join(" : "),
    };
    (signature, Some(current))
}
