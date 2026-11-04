//! Variable-module dispatch through a unique in-repository behaviour contract.
use super::common::edge;
use crate::{
    db::queries::QueryBuilder,
    extraction::grammars::get_parser,
    resolution::types::ResolutionContext,
    types::{Edge, EdgeKind, Language, NodeKind},
    web_tree_sitter::SyntaxNode,
};
use serde_json::json;
use std::collections::{HashMap, HashSet};

fn atom(node: &SyntaxNode) -> String {
    node.text.trim_matches('\'').to_owned()
}
fn arity(node: &SyntaxNode) -> usize {
    node.named_children
        .iter()
        .filter(|n| n.node_type() != "comment")
        .count()
}
fn visit<'a>(node: &'a SyntaxNode, sites: &mut Vec<&'a SyntaxNode>) {
    // Type-position calls, macro replacement bodies and comments are not dispatch sites.
    if matches!(
        node.node_type(),
        "spec" | "callback" | "type_alias" | "opaque" | "pp_define" | "record_decl"
    ) {
        return;
    }
    if node.node_type() == "remote" && !node.has_error {
        sites.push(node);
    }
    for child in &node.named_children {
        visit(child, sites);
    }
}

pub(super) fn behaviour_edges(
    queries: &mut QueryBuilder,
    ctx: &mut dyn ResolutionContext,
) -> Vec<Edge> {
    let modules: Vec<_> = ctx
        .get_nodes_by_kind(NodeKind::Namespace)
        .into_iter()
        .filter(|n| n.language == Language::Erlang)
        .collect();
    if modules.is_empty() {
        return vec![];
    }
    let Some(mut parser) = get_parser(Language::Erlang) else {
        return vec![];
    };
    let mut files = HashMap::new();
    let mut contracts: HashMap<String, HashSet<String>> = HashMap::new();
    for module in &modules {
        let Some(source) = ctx.read_file(&module.file_path) else {
            continue;
        };
        let Some(tree) = parser.parse(&source, None) else {
            continue;
        };
        for form in &tree.root_node.named_children {
            if form.node_type() != "callback" || form.has_error {
                continue;
            }
            let Some(fun) = form
                .child_for_field_name("fun")
                .filter(|n| n.node_type() == "atom")
            else {
                continue;
            };
            for sig in form
                .named_children
                .iter()
                .filter(|n| n.node_type() == "type_sig")
            {
                if let Some(args) = sig.child_for_field_name("args") {
                    contracts
                        .entry(format!("{}/{}", atom(fun), arity(args)))
                        .or_default()
                        .insert(module.id.clone());
                }
            }
        }
        files.insert(module.file_path.clone(), tree);
    }
    let mut edges = vec![];
    for (file, tree) in files {
        let nodes = ctx.get_nodes_in_file(&file);
        let mut sites = vec![];
        visit(&tree.root_node, &mut sites);
        for site in sites {
            let Some(module) = site
                .child_for_field_name("module")
                .and_then(|n| n.child_for_field_name("module"))
                .filter(|n| n.node_type() == "var")
            else {
                continue;
            };
            if module.text == "_" {
                continue;
            }
            let Some(call) = site
                .child_for_field_name("fun")
                .filter(|n| n.node_type() == "call")
            else {
                continue;
            };
            let Some(fun) = call
                .child_for_field_name("expr")
                .filter(|n| n.node_type() == "atom")
            else {
                continue;
            };
            let Some(args) = call.child_for_field_name("args") else {
                continue;
            };
            let key = format!("{}/{}", atom(fun), arity(args));
            let Some(behaviours) = contracts.get(&key).filter(|b| b.len() == 1) else {
                continue;
            };
            let behaviour = modules.iter().find(|n| behaviours.contains(&n.id)).unwrap();
            // A duplicate module name is ambiguous even if only one copy declares the callback.
            if modules.iter().filter(|n| n.name == behaviour.name).count() != 1 {
                continue;
            }
            let line = site.start_position.row as u64 + 1;
            let Some(owner) = nodes
                .iter()
                .filter(|n| {
                    n.kind == NodeKind::Function && n.start_line <= line && n.end_line >= line
                })
                .min_by_key(|n| n.end_line - n.start_line)
            else {
                continue;
            };
            let mut targets = vec![];
            for implementation in queries
                .get_incoming_edges(&behaviour.id, Some(vec![EdgeKind::Implements]))
                .unwrap_or_default()
            {
                let Some(implementer) = modules.iter().find(|n| n.id == implementation.source)
                else {
                    continue;
                };
                if modules
                    .iter()
                    .filter(|n| n.name == implementer.name)
                    .count()
                    != 1
                {
                    continue;
                }
                let matching: Vec<_> = ctx
                    .get_nodes_in_file(&implementer.file_path)
                    .into_iter()
                    .filter(|n| {
                        n.kind == NodeKind::Function
                            && n.is_exported == Some(true)
                            && n.qualified_name == format!("{}::{key}", implementer.name)
                    })
                    .collect();
                if matching.len() == 1 {
                    targets.extend(matching);
                }
            }
            targets.sort_by(|a, b| a.id.cmp(&b.id));
            targets.dedup_by(|a, b| a.id == b.id);
            if targets.len() > 24 {
                continue;
            }
            for target in targets {
                if target.id == owner.id {
                    continue;
                }
                edges.push(edge(
                    &owner.id,
                    &target.id,
                    EdgeKind::Calls,
                    Some(line),
                    "erlang-behaviour",
                    [
                        ("via", json!(format!("{}:{key}", behaviour.name))),
                        ("registeredAt", json!(format!("{file}:{line}"))),
                    ],
                ));
            }
        }
    }
    edges
}
