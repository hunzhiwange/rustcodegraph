//! CFML declarations from native tag and script ASTs.
use crate::extraction::tree_sitter_types::{ExtractorContext, LanguageExtractor, NodeExtra};
use crate::types::NodeKind;
use crate::web_tree_sitter::{SyntaxNode, parse_cfscript};
pub struct CfmlExtractor;
pub const CFML_EXTRACTOR: CfmlExtractor = CfmlExtractor;
impl LanguageExtractor for CfmlExtractor {
    fn visit_node(&self, node: &SyntaxNode, ctx: &mut dyn ExtractorContext) -> bool {
        if node.node_type() != "program" {
            return false;
        }
        if is_bare_script(ctx.source()) {
            if let Some(tree) = parse_cfscript(ctx.source()) {
                walk(&tree.root_node, ctx);
            }
        } else {
            walk(node, ctx);
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

fn attribute(node: &SyntaxNode, key: &str) -> Option<String> {
    for child in &node.named_children {
        if child.node_type() == "cf_attribute" {
            let name = child
                .named_children
                .iter()
                .find(|n| n.node_type() == "cf_attribute_name");
            if name.is_some_and(|n| n.text.eq_ignore_ascii_case(key)) {
                let value = child.named_children.iter().find(|n| {
                    matches!(
                        n.node_type(),
                        "quoted_cf_attribute_value" | "cf_attribute_value"
                    )
                })?;
                // Dynamic attributes are expressions, not stable symbol names.
                if value.text.contains('#') {
                    return None;
                }
                return Some(value.text.trim_matches(['\'', '"']).to_owned());
            }
        } else if child.node_type() == "cf_tag_attributes" {
            if let Some(value) = attribute(child, key) {
                return Some(value);
            }
        }
    }
    None
}

fn component_name(ctx: &dyn ExtractorContext) -> String {
    ctx.file_path()
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(ctx.file_path())
        .rsplit_once('.')
        .map(|(stem, _)| stem)
        .unwrap_or(ctx.file_path())
        .to_owned()
}

fn declaration(
    node: &SyntaxNode,
    ctx: &mut dyn ExtractorContext,
    kind: NodeKind,
    name: &str,
    exported: bool,
) {
    if name.is_empty() {
        return;
    }
    if let Some(created) = ctx.create_node(
        kind,
        name,
        node,
        NodeExtra {
            is_exported: Some(exported),
            ..Default::default()
        },
    ) {
        ctx.push_scope(created.id);
        for child in &node.named_children {
            walk(child, ctx);
        }
        ctx.pop_scope();
    }
}

fn walk(node: &SyntaxNode, ctx: &mut dyn ExtractorContext) {
    match node.node_type() {
        "comment" | "cf_comment" | "ERROR" => return,
        "component" => {
            let name = component_name(ctx);
            declaration(node, ctx, NodeKind::Class, &name, true);
            return;
        }
        "function_declaration" | "cf_function_tag" => {
            let tag = node.node_type() == "cf_function_tag";
            let name = if tag {
                attribute(node, "name")
            } else {
                node.child_for_field_name("name").map(|n| n.text.clone())
            };
            if let Some(name) = name {
                let access = if tag {
                    attribute(node, "access")
                } else {
                    node.named_children
                        .iter()
                        .find(|n| n.node_type() == "access_type")
                        .map(|n| n.text.clone())
                };
                let exported = !access.is_some_and(|a| {
                    matches!(a.to_ascii_lowercase().as_str(), "private" | "package")
                });
                let method = ctx
                    .node_stack()
                    .last()
                    .and_then(|id| ctx.nodes().iter().find(|n| &n.id == id))
                    .is_some_and(|n| n.kind == NodeKind::Class);
                declaration(
                    node,
                    ctx,
                    if method {
                        NodeKind::Method
                    } else {
                        NodeKind::Function
                    },
                    &name,
                    exported,
                );
            }
            return;
        }
        "cf_script_content" => {
            // Mask the prefix byte-for-byte to retain original rows, columns and UTF-8 offsets.
            let mut source: String = ctx.source().as_bytes()[..node.start_index]
                .iter()
                .map(|b| if *b == b'\n' { '\n' } else { ' ' })
                .collect();
            source.push_str(&node.text);
            if let Some(tree) = parse_cfscript(&source) {
                walk(&tree.root_node, ctx);
            }
            return;
        }
        _ => {}
    }
    let children = &node.named_children;
    let mut i = 0;
    while i < children.len() {
        let child = &children[i];
        if child.node_type() == "cf_component_open_tag" {
            let end = (i + 1..children.len())
                .find(|j| children[*j].node_type() == "cf_component_close_tag")
                .unwrap_or(children.len().saturating_sub(1));
            let mut extent = child.clone();
            extent.end_position = children[end].end_position;
            extent.end_index = children[end].end_index;
            let name = attribute(child, "name").unwrap_or_else(|| component_name(ctx));
            if let Some(created) = ctx.create_node(
                NodeKind::Class,
                &name,
                &extent,
                NodeExtra {
                    is_exported: Some(true),
                    ..Default::default()
                },
            ) {
                ctx.push_scope(created.id);
                for member in &children[i + 1..=end] {
                    walk(member, ctx);
                }
                ctx.pop_scope();
            }
            i = end + 1;
        } else {
            walk(child, ctx);
            i += 1;
        }
    }
}

/// Comments may precede the first script token in a CFC/template.
fn is_bare_script(source: &str) -> bool {
    let mut rest = source.trim_start_matches('\u{feff}').trim_start();
    loop {
        if let Some(comment) = rest.strip_prefix("//") {
            rest = comment
                .split_once('\n')
                .map(|(_, tail)| tail)
                .unwrap_or("")
                .trim_start();
        } else if let Some(comment) = rest.strip_prefix("/*") {
            rest = comment
                .split_once("*/")
                .map(|(_, tail)| tail)
                .unwrap_or("")
                .trim_start();
        } else {
            break;
        }
    }
    !rest.is_empty() && !rest.starts_with('<')
}
