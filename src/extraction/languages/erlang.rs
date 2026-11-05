//! Form-based Erlang declarations (native ELP grammar).
use crate::extraction::tree_sitter_helpers::{get_child_by_field, get_node_text};
use crate::extraction::tree_sitter_types::{
    ExtractorContext, LanguageExtractor, NodeExtra, UnresolvedReferenceInput,
};
use crate::types::{NodeKind, ReferenceKind};
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

fn reference(ctx: &mut dyn ExtractorContext, node: &SyntaxNode, name: String, kind: ReferenceKind) {
    if let Some(owner) = ctx.node_stack().last().cloned() {
        ctx.add_unresolved_reference(UnresolvedReferenceInput {
            from_node_id: owner,
            reference_name: name,
            reference_kind: kind,
            line: Some(node.start_position.row + 1),
            column: Some(node.start_position.column),
            file_path: None,
        });
    }
}

// Preserve the remote wrapper: the grammar puts the module outside `call`.
// Only atoms (and the compiler's current-module macro) have static targets.
fn module_prefix(node: &SyntaxNode, source: &str) -> Option<String> {
    if node.node_type() == "atom" {
        return Some(format!("{}::", atom(node, source)));
    }
    if node.node_type() == "macro_call_expr"
        && get_child_by_field(node, "name").is_some_and(|n| get_node_text(n, source) == "MODULE")
    {
        return Some(String::new());
    }
    None
}
fn walk_refs(node: &SyntaxNode, ctx: &mut dyn ExtractorContext, source: &str) {
    match node.node_type() {
        "spec" | "callback" | "type_alias" | "opaque" | "record_decl" => return,
        "remote" => {
            if let (Some(module), Some(call)) = (
                get_child_by_field(node, "module").and_then(|n| get_child_by_field(n, "module")),
                get_child_by_field(node, "fun"),
            ) && call.node_type() == "call"
            {
                emit_call(call, node, module_prefix(module, source), ctx, source);
                if let Some(args) = get_child_by_field(call, "args") {
                    walk_refs(args, ctx, source);
                }
                return;
            }
        }
        "call" => {
            emit_call(node, node, Some(String::new()), ctx, source);
        }
        "internal_fun" | "external_fun" => {
            let prefix = if node.node_type() == "external_fun" {
                get_child_by_field(node, "module")
                    .and_then(|n| get_child_by_field(n, "name"))
                    .and_then(|n| module_prefix(n, source))
            } else {
                Some(String::new())
            };
            if let (Some(prefix), Some(fun), Some(arity)) = (
                prefix,
                get_child_by_field(node, "fun").filter(|n| n.node_type() == "atom"),
                get_child_by_field(node, "arity")
                    .and_then(|n| get_child_by_field(n, "value"))
                    .filter(|n| n.node_type() == "integer"),
            ) {
                reference(
                    ctx,
                    node,
                    format!(
                        "{prefix}{}/{}",
                        atom(fun, source),
                        get_node_text(arity, source)
                    ),
                    ReferenceKind::References,
                );
            }
            return;
        }
        "macro_call_expr" => {
            if let Some(name) = get_child_by_field(node, "name") {
                let name = get_node_text(name, source);
                if !matches!(
                    name.as_str(),
                    "MODULE"
                        | "MODULE_STRING"
                        | "FILE"
                        | "LINE"
                        | "FUNCTION_NAME"
                        | "FUNCTION_ARITY"
                        | "MACHINE"
                        | "OTP_RELEASE"
                ) {
                    reference(
                        ctx,
                        node,
                        name,
                        if get_child_by_field(node, "args").is_some() {
                            ReferenceKind::Calls
                        } else {
                            ReferenceKind::References
                        },
                    );
                }
            }
        }
        _ => {}
    }
    for child in &node.named_children {
        walk_refs(child, ctx, source);
    }
}
fn emit_call(
    call: &SyntaxNode,
    position: &SyntaxNode,
    prefix: Option<String>,
    ctx: &mut dyn ExtractorContext,
    source: &str,
) {
    if let (Some(prefix), Some(fun), Some(args)) = (
        prefix,
        get_child_by_field(call, "expr").filter(|n| n.node_type() == "atom"),
        get_child_by_field(call, "args"),
    ) {
        let arity = args
            .named_children
            .iter()
            .filter(|n| n.node_type() != "comment")
            .count();
        reference(
            ctx,
            position,
            format!("{prefix}{}/{arity}", atom(fun, source)),
            ReferenceKind::Calls,
        );
    }
}
fn directive(form: &SyntaxNode, ctx: &mut dyn ExtractorContext, source: &str) {
    match form.node_type() {
        "behaviour_attribute" => {
            if let Some(name) = get_child_by_field(form, "name").filter(|n| n.node_type() == "atom")
            {
                reference(ctx, form, atom(name, source), ReferenceKind::Implements);
            }
        }
        "pp_include" | "pp_include_lib" | "import_attribute" => {
            let field = if form.node_type() == "import_attribute" {
                "module"
            } else {
                "file"
            };
            if let Some(name) = get_child_by_field(form, field)
                .filter(|n| matches!(n.node_type(), "atom" | "string"))
            {
                let name = get_node_text(name, source)
                    .trim_matches('"')
                    .trim_matches('\'')
                    .to_owned();
                reference(ctx, form, name, ReferenceKind::Imports);
            }
        }
        "pp_define" => {
            if let Some(name) =
                get_child_by_field(form, "lhs").and_then(|n| get_child_by_field(n, "name"))
                && let Some(n) = ctx.create_node(
                    NodeKind::Constant,
                    &get_node_text(name, source),
                    form,
                    NodeExtra::default(),
                )
            {
                ctx.push_scope(n.id);
                if let Some(body) = get_child_by_field(form, "replacement") {
                    walk_refs(body, ctx, source);
                }
                ctx.pop_scope();
            }
        }
        _ => {}
    }
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
            if form.node_type() == "module_attribute"
                && let Some(name) = get_child_by_field(form, "name")
            {
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
            if form.node_type() == "export_attribute" {
                for fa in &form.named_children {
                    if fa.node_type() != "fa" {
                        continue;
                    }
                    if let (Some(name), Some(arity)) = (
                        get_child_by_field(fa, "fun"),
                        get_child_by_field(fa, "arity")
                            .and_then(|n| get_child_by_field(n, "value")),
                    ) && let Ok(arity) = get_node_text(arity, &source).parse::<usize>()
                    {
                        exports.insert((atom(name, &source), arity));
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
                directive(form, ctx, &source);
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
            let function = ctx.create_node(
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
            if let Some(function) = function {
                ctx.push_scope(function.id);
                for clause in &forms[i..j] {
                    walk_refs(clause, ctx, &source);
                }
                ctx.pop_scope();
            }
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
