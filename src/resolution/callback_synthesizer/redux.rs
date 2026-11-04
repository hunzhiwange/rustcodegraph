//! Registered RTK thunk chains. Unlike upstream's global name fallback, require
//! an imported factory and a unique local/imported registered thunk target.
use super::common::edge;
use crate::{
    resolution::types::ResolutionContext,
    types::{Edge, EdgeKind, Language, NodeKind},
    web_tree_sitter::{Language as Grammar, Parser, SyntaxNode},
};
use serde_json::json;
use std::collections::HashSet;

fn field_text(node: &SyntaxNode, field: &str) -> Option<String> {
    node.child_for_field_name(field).map(|n| n.text())
}

// Do not attribute deferred nested callbacks to the thunk being executed.
fn dispatches(node: &SyntaxNode, output: &mut Vec<(String, u64)>) {
    if matches!(
        node.node_type(),
        "arrow_function"
            | "function_expression"
            | "function_declaration"
            | "method_definition"
            | "class_declaration"
    ) {
        return;
    }
    if node.node_type() == "call_expression"
        && field_text(node, "function").as_deref() == Some("dispatch")
    {
        if let Some(call) = node
            .child_for_field_name("arguments")
            .and_then(|a| a.named_children.first())
        {
            if call.node_type() == "call_expression" {
                if let Some(name) = call
                    .child_for_field_name("function")
                    .filter(|n| n.node_type() == "identifier")
                {
                    output.push((name.text(), node.start_position.row as u64 + 1));
                }
            }
        }
    }
    for child in &node.named_children {
        dispatches(child, output);
    }
}

pub(super) fn redux_thunk_edges(ctx: &mut dyn ResolutionContext) -> Vec<Edge> {
    let mut registered = Vec::new();
    for file in ctx.get_all_files() {
        let nodes = ctx.get_nodes_in_file(&file);
        let Some(language) = nodes.first().map(|n| n.language).filter(|l| {
            matches!(
                l,
                Language::TypeScript | Language::Tsx | Language::JavaScript | Language::Jsx
            )
        }) else {
            continue;
        };
        let imports = ctx.get_import_mappings(&file, language);
        let factories: HashSet<_> = imports
            .iter()
            .filter(|m| m.source == "@reduxjs/toolkit" && m.exported_name == "createAsyncThunk")
            .map(|m| m.local_name.clone())
            .collect();
        if factories.is_empty() {
            continue;
        }
        let Some(source) = ctx.read_file(&file) else {
            continue;
        };
        let Ok(grammar) = Grammar::for_code_language(language) else {
            continue;
        };
        let mut parser = Parser::default();
        parser.set_language(Some(grammar));
        let Some(tree) = parser.parse(&source, None) else {
            continue;
        };
        for decl in tree.root_node.descendants_of_type(&["variable_declarator"]) {
            let Some(name) = field_text(decl, "name") else {
                continue;
            };
            let Some(value) = decl
                .child_for_field_name("value")
                .filter(|v| v.node_type() == "call_expression")
            else {
                continue;
            };
            if !field_text(value, "function").is_some_and(|f| factories.contains(&f)) {
                continue;
            }
            let Some(callback) = value
                .child_for_field_name("arguments")
                .and_then(|a| a.named_children.get(1))
                .filter(|n| matches!(n.node_type(), "arrow_function" | "function_expression"))
            else {
                continue;
            };
            let Some(body) = callback.child_for_field_name("body") else {
                continue;
            };
            let has_dispatch = callback
                .child_for_field_name("parameters")
                .and_then(|p| p.named_children.get(1))
                .is_some_and(|p| {
                    p.descendants_of_type(&["shorthand_property_identifier_pattern"])
                        .iter()
                        .any(|n| n.text() == "dispatch")
                });
            let candidates: Vec<_> = nodes
                .iter()
                .filter(|n| {
                    n.name == name
                        && matches!(n.kind, NodeKind::Constant | NodeKind::Variable)
                        && n.start_line <= decl.start_position.row as u64 + 1
                        && n.end_line >= decl.end_position.row as u64 + 1
                })
                .collect();
            if candidates.len() != 1 {
                continue;
            }
            let mut calls = Vec::new();
            if has_dispatch {
                dispatches(body, &mut calls);
            }
            // Any local binding of a dispatched name makes this conservative
            // pass abstain; a nested scope must never select a global thunk.
            let bindings =
                body.descendants_of_type(&["variable_declarator", "function_declaration"]);
            calls.retain(|(name, _)| {
                !bindings
                    .iter()
                    .any(|n| field_text(n, "name").is_some_and(|b| b == *name || b == "dispatch"))
            });
            if let Some(parameters) = callback.child_for_field_name("parameters") {
                let identifiers = parameters
                    .descendants_of_type(&["identifier", "shorthand_property_identifier_pattern"]);
                calls.retain(|(name, _)| !identifiers.iter().any(|n| n.text() == *name));
            }
            registered.push((candidates[0].clone(), calls));
        }
    }
    let mut edges = Vec::new();
    for (source, calls) in &registered {
        let mut imports = ctx.get_import_mappings(&source.file_path, source.language);
        for mapping in &mut imports {
            if mapping.resolved_path.is_none() {
                mapping.resolved_path = crate::resolution::import_resolver::resolve_import_path(
                    &mapping.source,
                    &source.file_path,
                    source.language,
                    ctx,
                );
            }
        }
        let mut seen = HashSet::new();
        for (name, line) in calls {
            let targets: Vec<_> = registered
                .iter()
                .filter(|(target, _)| {
                    (target.file_path == source.file_path && target.name == *name)
                        || imports.iter().any(|m| {
                            m.local_name == *name
                                && m.exported_name == target.name
                                && m.resolved_path.as_deref() == Some(target.file_path.as_str())
                        })
                })
                .collect();
            if targets.len() != 1 {
                continue;
            }
            let target = &targets[0].0;
            if source.id == target.id || !seen.insert(target.id.clone()) || seen.len() > 24 {
                continue;
            }
            edges.push(edge(
                &source.id,
                &target.id,
                EdgeKind::Calls,
                Some(*line),
                "redux-thunk",
                [
                    ("via", json!(name)),
                    (
                        "registeredAt",
                        json!(format!("{}:{}", source.file_path, line)),
                    ),
                ],
            ));
        }
    }
    edges
}
