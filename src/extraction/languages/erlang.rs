//! Form-based Erlang declarations (native ELP grammar).
use crate::extraction::tree_sitter_helpers::{get_child_by_field, get_node_text};
use crate::extraction::tree_sitter_types::{ExtractorContext, LanguageExtractor, NodeExtra};
use crate::types::NodeKind;
use crate::web_tree_sitter::SyntaxNode;
use std::collections::HashSet;

fn atom(node: &SyntaxNode, source: &str) -> String {
    get_node_text(node, source).trim_matches('\'').to_owned()
}
fn identity(node: &SyntaxNode, source: &str) -> Option<(String, usize)> {
    let clause = node
        .named_children
        .iter()
        .find(|c| c.node_type() == "function_clause")?;
    let name = get_child_by_field(clause, "name")?;
    if name.node_type() != "atom" {
        return None;
    }
    let args = get_child_by_field(clause, "args")?;
    Some((
        atom(name, source),
        args.named_children
            .iter()
            .filter(|c| c.node_type() != "comment")
            .count(),
    ))
}

pub struct ErlangExtractor;
pub const ERLANG_EXTRACTOR: ErlangExtractor = ErlangExtractor;
impl LanguageExtractor for ErlangExtractor {
    fn visit_node(&self, root: &SyntaxNode, ctx: &mut dyn ExtractorContext) -> bool {
        if root.node_type() != "source_file" {
            return false;
        }
        let source = ctx.source().to_owned();
        let mut module = None;
        let mut exports = HashSet::new();
        let mut export_all = false;
        for form in &root.named_children {
            if form.node_type() == "module_attribute" {
                if let Some(name) = get_child_by_field(form, "name") {
                    let name = atom(name, &source);
                    if let Some(n) = ctx.create_node(
                        NodeKind::Namespace,
                        &name,
                        form,
                        NodeExtra {
                            qualified_name: Some(name.clone()),
                            ..Default::default()
                        },
                    ) {
                        module = Some((name, n.id));
                    }
                }
            }
            if form.node_type() == "export_attribute" {
                for fa in &form.named_children {
                    if fa.node_type() != "fa" {
                        continue;
                    }
                    if let (Some(name), Some(arity)) = (
                        get_child_by_field(fa, "fun"),
                        get_child_by_field(fa, "arity")
                            .and_then(|n| get_child_by_field(n, "value")),
                    ) {
                        if let Ok(arity) = get_node_text(arity, &source).parse::<usize>() {
                            exports.insert((atom(name, &source), arity));
                        }
                    }
                }
            }
            if form.node_type() == "compile_options_attribute" {
                // Only an AST atom enables export_all, never comments or strings.
                fn has_export_all(n: &SyntaxNode, source: &str) -> bool {
                    (n.node_type() == "atom" && atom(n, source) == "export_all")
                        || n.named_children.iter().any(|c| has_export_all(c, source))
                }
                export_all |= has_export_all(form, &source);
            }
        }
        if let Some((_, id)) = &module {
            ctx.push_scope(id.clone());
        }
        let forms = &root.named_children;
        let mut i = 0;
        while i < forms.len() {
            let form = &forms[i];
            let Some((name, arity)) =
                identity(form, &source).filter(|_| form.node_type() == "fun_decl")
            else {
                i += 1;
                continue;
            };
            let mut span = form.clone();
            let mut j = i + 1;
            while j < forms.len() {
                if forms[j].node_type() == "comment" {
                    j += 1;
                    continue;
                }
                if forms[j].node_type() != "fun_decl"
                    || identity(&forms[j], &source) != Some((name.clone(), arity))
                {
                    break;
                }
                span.end_index = forms[j].end_index;
                span.end_position = forms[j].end_position;
                j += 1;
            }
            let qualified = match &module {
                Some((m, _)) => format!("{m}::{name}/{arity}"),
                None => format!("{name}/{arity}"),
            };
            ctx.create_node(
                NodeKind::Function,
                &name,
                &span,
                NodeExtra {
                    qualified_name: Some(qualified),
                    is_exported: Some(export_all || exports.contains(&(name.clone(), arity))),
                    signature: Some(
                        get_node_text(form, &source)
                            .split("->")
                            .next()
                            .unwrap_or("")
                            .split_whitespace()
                            .collect::<Vec<_>>()
                            .join(" "),
                    ),
                    ..Default::default()
                },
            );
            i = j;
        }
        if module.is_some() {
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
        "args"
    }
}
