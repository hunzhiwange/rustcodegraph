//! Terraform declarations from native HCL blocks; expressions remain opaque.
use crate::extraction::tree_sitter_helpers::get_node_text;
use crate::extraction::tree_sitter_types::{ExtractorContext, LanguageExtractor, NodeExtra};
use crate::types::NodeKind;
use crate::web_tree_sitter::SyntaxNode;

pub struct TerraformExtractor;
pub const TERRAFORM_EXTRACTOR: TerraformExtractor = TerraformExtractor;
impl LanguageExtractor for TerraformExtractor {
    fn visit_node(&self, node: &SyntaxNode, ctx: &mut dyn ExtractorContext) -> bool {
        if node.node_type() != "config_file" {
            return false;
        }
        // Only root blocks declare Terraform addresses. Never recurse into
        // nested blocks, object keys, comprehensions or arbitrary attributes.
        for body in &node.named_children {
            if body.node_type() != "body" {
                continue;
            }
            for block in &body.named_children {
                if block.node_type() == "block" {
                    extract_block(block, ctx);
                }
            }
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

fn extract_block(block: &SyntaxNode, ctx: &mut dyn ExtractorContext) {
    let mut children = block.named_children.iter();
    let Some(head) = children.next().filter(|n| n.node_type() == "identifier") else {
        return;
    };
    let block_type = get_node_text(head, ctx.source()).to_owned();
    let mut labels = Vec::new();
    for child in children {
        let value = match child.node_type() {
            "identifier" => get_node_text(child, ctx.source()).to_owned(),
            "string_lit" => child
                .named_children
                .iter()
                .find(|n| n.node_type() == "template_literal")
                .map(|n| get_node_text(n, ctx.source()).to_owned())
                .unwrap_or_default(),
            _ => break,
        };
        labels.push(value);
    }
    if block_type == "locals" && labels.is_empty() {
        if let Some(body) = block
            .named_children
            .iter()
            .find(|n| n.node_type() == "body")
        {
            for attr in &body.named_children {
                if attr.node_type() != "attribute" {
                    continue;
                }
                if let Some(id) = attr
                    .named_children
                    .iter()
                    .find(|n| n.node_type() == "identifier")
                {
                    let name = get_node_text(id, ctx.source()).to_owned();
                    let address = format!("local.{name}");
                    emit(
                        ctx,
                        attr,
                        NodeKind::Constant,
                        &name,
                        address.clone(),
                        address,
                        false,
                    );
                }
            }
        }
        return;
    }
    if labels.iter().any(|s| s.is_empty()) {
        return;
    }
    let (kind, name, address, exported) = match (block_type.as_str(), labels.as_slice()) {
        ("resource", [ty, name]) => (
            NodeKind::Class,
            format!("{ty}.{name}"),
            format!("{ty}.{name}"),
            false,
        ),
        ("data", [ty, name]) => (
            NodeKind::Class,
            format!("{ty}.{name}"),
            format!("data.{ty}.{name}"),
            false,
        ),
        ("module", [name]) => (
            NodeKind::Module,
            name.clone(),
            format!("module.{name}"),
            false,
        ),
        ("variable", [name]) => (
            NodeKind::Variable,
            name.clone(),
            format!("var.{name}"),
            true,
        ),
        ("output", [name]) => (
            NodeKind::Variable,
            name.clone(),
            format!("output.{name}"),
            true,
        ),
        _ => return,
    };
    let signature = format!(
        "{} {}",
        block_type,
        labels
            .iter()
            .map(|s| format!("\"{s}\""))
            .collect::<Vec<_>>()
            .join(" ")
    );
    emit(ctx, block, kind, &name, address, signature, exported);
}

fn emit(
    ctx: &mut dyn ExtractorContext,
    node: &SyntaxNode,
    kind: NodeKind,
    name: &str,
    address: String,
    signature: String,
    exported: bool,
) {
    ctx.create_node(
        kind,
        name,
        node,
        NodeExtra {
            qualified_name: Some(address),
            signature: Some(signature),
            is_exported: Some(exported),
            ..NodeExtra::default()
        },
    );
}
