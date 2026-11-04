//! COBOL declarations: procedure headers are flat siblings in the native AST.
use crate::extraction::tree_sitter_types::{ExtractorContext, LanguageExtractor, NodeExtra};
use crate::types::NodeKind;
use crate::web_tree_sitter::SyntaxNode;
pub struct CobolExtractor;
pub const COBOL_EXTRACTOR: CobolExtractor = CobolExtractor;
impl LanguageExtractor for CobolExtractor {
    fn visit_node(&self, node: &SyntaxNode, ctx: &mut dyn ExtractorContext) -> bool {
        match node.node_type() {
            "program_definition" => {
                let name = node
                    .named_children
                    .iter()
                    .find(|n| n.node_type() == "identification_division")
                    .and_then(|n| {
                        n.named_children
                            .iter()
                            .find(|n| n.node_type() == "program_name")
                    })
                    .map(|n| n.text.trim().trim_matches(['\'', '"']).to_owned());
                let created = name.filter(|name| !name.is_empty()).and_then(|name| {
                    ctx.create_node(NodeKind::Module, &name, node, NodeExtra::default())
                });
                if let Some(ref created) = created {
                    ctx.push_scope(created.id.clone());
                }
                for child in &node.named_children {
                    ctx.visit_node(child);
                }
                if created.is_some() {
                    ctx.pop_scope();
                }
                true
            }
            "procedure_division" | "copybook_fragment" => {
                walk_procedure(node, ctx);
                true
            }
            _ => false,
        }
    }
    fn pre_parse(&self, source: &str) -> Option<String> {
        static MARKER: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
            regex::Regex::new(r"(?i)^([ \t]*)(IDENTIFICATION\s+DIVISION|ID\s+DIVISION|PROGRAM-ID\b|\d{2}[ \t]+[A-Za-z])").unwrap()
        });
        let free = source
            .lines()
            .find_map(|line| MARKER.captures(line).map(|m| m[1].len() < 7))
            .unwrap_or(false);
        if !free {
            return None;
        }
        // Preserve lines; enable the patched scanner's wide free-format margin.
        Some(
            source
                .split('\n')
                .enumerate()
                .map(|(i, line)| {
                    if i == 0 {
                        format!("CGWIDE {line}")
                    } else if line.is_empty() {
                        String::new()
                    } else {
                        format!("       {line}")
                    }
                })
                .collect::<Vec<_>>()
                .join("\n"),
        )
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

/// Sections own paragraphs, while adjacent paragraphs remain siblings. Extents
/// stop immediately before the next relevant header, including intervening lines.
fn walk_procedure(node: &SyntaxNode, ctx: &mut dyn ExtractorContext) {
    let children = &node.named_children;
    let mut section_open = false;
    for (i, child) in children.iter().enumerate() {
        let section = child.node_type() == "section_header";
        if !section && child.node_type() != "paragraph_header" {
            continue;
        }
        if section && section_open {
            ctx.pop_scope();
            section_open = false;
        }
        let name = child
            .text
            .split_whitespace()
            .next()
            .unwrap_or("")
            .trim_end_matches('.');
        if name.is_empty() {
            continue;
        }
        let mut region = child.clone();
        if let Some(next) = children.iter().skip(i + 1).find(|n| {
            n.node_type() == "section_header" || (!section && n.node_type() == "paragraph_header")
        }) {
            region.end_position.row = next
                .start_position
                .row
                .saturating_sub(1)
                .max(child.start_position.row);
            region.end_position.column = 0;
            region.end_index = next.start_index;
        } else {
            region.end_position = node.end_position;
            region.end_index = node.end_index;
        }
        let created = ctx.create_node(
            NodeKind::Function,
            name,
            &region,
            NodeExtra {
                signature: section.then(|| "SECTION".to_owned()),
                ..NodeExtra::default()
            },
        );
        if section && let Some(created) = created {
            ctx.push_scope(created.id);
            section_open = true;
        }
    }
    if section_open {
        ctx.pop_scope();
    }
}
