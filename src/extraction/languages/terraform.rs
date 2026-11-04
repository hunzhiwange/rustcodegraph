//! Terraform declarations and syntax-backed pending references from native HCL.
use crate::extraction::tree_sitter_helpers::get_node_text;
use crate::extraction::tree_sitter_types::{
    ExtractorContext, LanguageExtractor, NodeExtra, UnresolvedReferenceInput,
};
use crate::types::{NodeKind, ReferenceKind};
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
    let created = ctx.create_node(
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
    if let Some(created) = created {
        if let Some(body) = node.named_children.iter().find(|n| n.node_type() == "body") {
            for attr in &body.named_children {
                let key = attr
                    .named_children
                    .iter()
                    .find(|n| n.node_type() == "identifier")
                    .map(|n| get_node_text(n, ctx.source()))
                    .unwrap_or_default();
                if kind == NodeKind::Module && attr.node_type() == "attribute" {
                    let reference = if key == "source" {
                        // Only a direct literal source is a statically known module boundary.
                        attr.named_children
                            .iter()
                            .find(|n| n.node_type() == "expression")
                            .filter(|n| n.named_children.len() == 1)
                            .and_then(|n| n.named_children.first())
                            .filter(|n| {
                                n.node_type() == "literal_value" && n.named_children.len() == 1
                            })
                            .and_then(|n| n.named_children.first())
                            .filter(|n| n.node_type() == "string_lit")
                            .map(|n| get_node_text(n, ctx.source()).trim_matches('"').to_owned())
                            .filter(|s| {
                                (s.starts_with("./") || s.starts_with("../"))
                                    && !s.contains("${")
                                    && !s.contains("%{")
                            })
                            .map(|s| {
                                (
                                    format!("{}:file:{s}", created.qualified_name),
                                    ReferenceKind::Imports,
                                )
                            })
                    } else if !["version", "count", "for_each", "providers", "depends_on"]
                        .contains(&key.as_str())
                    {
                        Some((
                            format!("{}:var.{key}", created.qualified_name),
                            ReferenceKind::References,
                        ))
                    } else {
                        None
                    };
                    if let Some((reference_name, reference_kind)) = reference {
                        ctx.add_unresolved_reference(UnresolvedReferenceInput {
                            from_node_id: created.id.clone(),
                            reference_name,
                            reference_kind,
                            line: Some(attr.start_position.row + 1),
                            column: Some(attr.start_position.column),
                            file_path: None,
                        });
                    }
                }
                let provider = kind == NodeKind::Class && key == "provider";
                let providers = kind == NodeKind::Module && key == "providers";
                if attr.node_type() == "attribute" && (provider || providers) {
                    if let Some(expr) = attr
                        .named_children
                        .iter()
                        .find(|n| n.node_type() == "expression")
                    {
                        if provider {
                            emit_provider(expr, ctx, &created.id);
                        } else {
                            collect_refs(expr, ctx, &created.id, true);
                        }
                    }
                } else {
                    collect_refs(attr, ctx, &created.id, false);
                }
            }
        } else {
            collect_refs(node, ctx, &created.id, false);
        }
    }
}

// Walk sibling traversals, rather than source text, so literal strings and
// comments cannot create dependencies. Nested blocks retain their declaration owner.
fn collect_refs(
    node: &SyntaxNode,
    ctx: &mut dyn ExtractorContext,
    owner: &str,
    provider_map: bool,
) {
    if provider_map && node.node_type() == "object_elem" {
        if let Some(value) = node.child_for_field_name("val") {
            emit_provider(value, ctx, owner);
        }
        return;
    }
    for (i, child) in node.named_children.iter().enumerate() {
        if child.node_type() == "variable_expr" {
            let Some(id) = child.named_children.first() else {
                continue;
            };
            let head = get_node_text(id, ctx.source());
            if matches!(
                head.as_str(),
                "each" | "count" | "self" | "path" | "terraform" | "null" | "true" | "false"
            ) {
                continue;
            }
            let mut attrs = Vec::new();
            for sibling in &node.named_children[i + 1..] {
                match sibling.node_type() {
                    "get_attr" => {
                        if let Some(id) = sibling.named_children.first() {
                            attrs.push(get_node_text(id, ctx.source()));
                        }
                    }
                    "index" | "new_index" | "legacy_index" | "splat" | "attr_splat"
                    | "full_splat" => {}
                    _ => break,
                }
            }
            let mut names = Vec::new();
            match (head.as_str(), attrs.as_slice()) {
                ("data", [ty, name, ..]) => names.push(format!("data.{ty}.{name}")),
                ("data", _) => {}
                ("module", [name, rest @ ..]) => {
                    names.push(format!("module.{name}"));
                    if let Some(output) = rest.first() {
                        names.push(format!("module.{name}:output.{output}"));
                    }
                    if let [outputs, output, ..] = rest {
                        if outputs == "outputs" {
                            names.push(format!("module.{name}:remote-output.{output}"));
                        }
                    }
                }
                (_, [name, ..]) => names.push(format!("{head}.{name}")),
                _ => {}
            }
            for name in names {
                pending(ctx, owner, name, child);
            }
        }
        collect_refs(child, ctx, owner, provider_map);
    }
}

fn emit_provider(expr: &SyntaxNode, ctx: &mut dyn ExtractorContext, owner: &str) {
    // Accept only a bare configuration name or a single static alias.
    let children = &expr.named_children;
    if !matches!(children.len(), 1 | 2) || children[0].node_type() != "variable_expr" {
        return;
    }
    let Some(id) = children[0].named_children.first() else {
        return;
    };
    let mut name = get_node_text(id, ctx.source());
    if children.len() == 2 {
        if children[1].node_type() != "get_attr" {
            return;
        }
        let Some(alias) = children[1].named_children.first() else {
            return;
        };
        name.push('.');
        name.push_str(&get_node_text(alias, ctx.source()));
    }
    pending(ctx, owner, format!("provider.{name}"), expr);
}

fn pending(ctx: &mut dyn ExtractorContext, owner: &str, name: String, node: &SyntaxNode) {
    ctx.add_unresolved_reference(UnresolvedReferenceInput {
        from_node_id: owner.to_owned(),
        reference_name: name,
        reference_kind: ReferenceKind::References,
        line: Some(node.start_position.row + 1),
        column: Some(node.start_position.column),
        file_path: None,
    });
}
