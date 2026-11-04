//! COBOL declarations: procedure headers are flat siblings in the native AST.
use crate::extraction::tree_sitter_types::{
    ExtractorContext, LanguageExtractor, NodeExtra, UnresolvedReferenceInput,
};
use crate::types::{NodeKind, ReferenceKind};
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
            "perform_statement_call_proc" | "call_statement" | "copy_statement" => {
                collect_refs(node, ctx);
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
    let mut current = ctx.node_stack().last().cloned();
    for (i, child) in children.iter().enumerate() {
        let section = child.node_type() == "section_header";
        if !section && child.node_type() != "paragraph_header" {
            if let Some(ref owner) = current {
                ctx.push_scope(owner.clone());
                collect_refs(child, ctx);
                ctx.pop_scope();
            }
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
        if let Some(ref created) = created {
            current = Some(created.id.clone());
        }
        if section && let Some(created) = created {
            ctx.push_scope(created.id);
            section_open = true;
        }
    }
    if section_open {
        ctx.pop_scope();
    }
}

/// Emit only AST-backed static targets; final resolution remains a separate phase.
fn collect_refs(node: &SyntaxNode, ctx: &mut dyn ExtractorContext) {
    match node.node_type() {
        "perform_statement_call_proc" => {
            if let Some(proc) = node.child_for_field_name("procedure") {
                for label in &proc.named_children {
                    if label.node_type() == "label" {
                        add_ref(label, label.text.trim(), ReferenceKind::Calls, ctx);
                    }
                }
            }
        }
        "call_statement" => {
            if let Some(target) = node.child_for_field_name("x") {
                if target.node_type() == "string" {
                    add_ref(
                        target,
                        target.text.trim_matches(['\'', '"']).trim(),
                        ReferenceKind::Calls,
                        ctx,
                    );
                }
            }
            // Exception clauses can contain further statements.
            for child in &node.named_children {
                collect_refs(child, ctx);
            }
        }
        "copy_statement" => {
            if let Some(book) = node.child_for_field_name("book") {
                let name = book.text.trim().trim_matches(['\'', '"']);
                if !name.is_empty() {
                    ctx.create_node(
                        NodeKind::Import,
                        name,
                        node,
                        NodeExtra {
                            signature: Some(
                                node.text.split_whitespace().collect::<Vec<_>>().join(" "),
                            ),
                            ..NodeExtra::default()
                        },
                    );
                    add_ref(node, name, ReferenceKind::Imports, ctx);
                }
            }
        }
        _ => {
            for child in &node.named_children {
                collect_refs(child, ctx);
            }
        }
    }
}

fn add_ref(at: &SyntaxNode, name: &str, kind: ReferenceKind, ctx: &mut dyn ExtractorContext) {
    if !name.is_empty()
        && let Some(owner) = ctx.node_stack().last().cloned()
    {
        ctx.add_unresolved_reference(UnresolvedReferenceInput {
            from_node_id: owner,
            reference_name: name.to_owned(),
            reference_kind: kind,
            line: Some(at.start_position.row + 1),
            column: Some(at.start_position.column),
            file_path: None,
        });
    }
}
