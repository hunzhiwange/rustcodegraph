//! ArkUI reactive writes -> build, followed by existing static component calls.
//! Source contract: codegraph 15483713, arkuiStateBuildEdges. Use the native
//! AST to avoid treating comments, strings or comparisons as state writes.
use super::common::{MAX_CALLBACKS_PER_CHANNEL, children_of_kind, edge};
use crate::{
    db::queries::QueryBuilder,
    resolution::types::ResolutionContext,
    types::{Edge, EdgeKind, Language, NodeKind},
    web_tree_sitter::{Language as Grammar, Parser, SyntaxNode},
};
use serde_json::json;
use std::collections::{HashMap, HashSet};

fn has_decorator(node: &SyntaxNode, names: &[&str]) -> bool {
    let matches = |n: &SyntaxNode| {
        n.node_type() == "decorator"
            && n.named_children.iter().any(|n| {
                if n.node_type() == "identifier" {
                    names.contains(&n.text().as_str())
                } else if n.node_type() == "call_expression" {
                    n.child_for_field_name("function").is_some_and(|f| {
                        f.node_type() == "identifier" && names.contains(&f.text().as_str())
                    })
                } else {
                    false
                }
            })
    };
    if node.named_children.iter().any(matches) {
        return true;
    }
    if node.parent().is_some_and(|p| {
        p.node_type() == "export_statement" && p.named_children.iter().any(matches)
    }) {
        return true;
    }
    let mut previous = node.previous_named_sibling();
    while let Some(sibling) = previous {
        if sibling.node_type() != "decorator" {
            break;
        }
        if matches(&sibling) {
            return true;
        }
        previous = sibling.previous_named_sibling();
    }
    false
}

fn own_property(node: &SyntaxNode, props: &HashSet<String>) -> bool {
    node.node_type() == "member_expression"
        && node
            .child_for_field_name("object")
            .is_some_and(|n| n.node_type() == "this")
        && node
            .child_for_field_name("property")
            .is_some_and(|n| props.contains(&n.text()))
}

fn writes(node: &SyntaxNode, props: &HashSet<String>) -> bool {
    // Nested functions have a different invocation/receiver boundary. Do not
    // attribute their writes to the enclosing method without dataflow evidence.
    if matches!(
        node.node_type(),
        "function_expression"
            | "function_declaration"
            | "arrow_function"
            | "class_declaration"
            | "method_definition"
            | "generator_function"
            | "generator_function_declaration"
    ) {
        return false;
    }
    let direct = match node.node_type() {
        "assignment_expression" | "augmented_assignment_expression" => node
            .child_for_field_name("left")
            .is_some_and(|n| own_property(n, props)),
        "update_expression" => node
            .child_for_field_name("argument")
            .is_some_and(|n| own_property(n, props)),
        "call_expression" => node.child_for_field_name("function").is_some_and(|callee| {
            callee.node_type() == "member_expression"
                && callee
                    .child_for_field_name("object")
                    .is_some_and(|n| own_property(n, props))
                && callee.child_for_field_name("property").is_some_and(|n| {
                    matches!(
                        n.text().as_str(),
                        "push"
                            | "pop"
                            | "shift"
                            | "unshift"
                            | "splice"
                            | "sort"
                            | "reverse"
                            | "fill"
                    )
                })
        }),
        _ => false,
    };
    direct || node.named_children.iter().any(|n| writes(n, props))
}

pub(super) fn state_build_edges(
    queries: &mut QueryBuilder,
    ctx: &mut dyn ResolutionContext,
) -> Vec<Edge> {
    let structs: Vec<_> = queries
        .get_nodes_by_kind(NodeKind::Struct)
        .unwrap_or_default()
        .into_iter()
        .filter(|n| n.language == Language::Arkts)
        .collect();
    if structs.is_empty() {
        return Vec::new();
    }
    let Ok(grammar) = Grammar::for_code_language(Language::Arkts) else {
        return Vec::new();
    };
    let mut parser = Parser::default();
    parser.set_language(Some(grammar));
    let mut edges = Vec::new();
    let mut trees = HashMap::new();
    for owner in structs {
        if !trees.contains_key(&owner.file_path) {
            let Some(source) = ctx.read_file(&owner.file_path) else {
                continue;
            };
            let Some(tree) = parser.parse(&source, None) else {
                continue;
            };
            trees.insert(owner.file_path.clone(), tree);
        }
        let tree = &trees[&owner.file_path];
        let owners: Vec<_> = tree
            .root_node
            .descendants_of_type(&["struct_declaration"])
            .into_iter()
            .filter(|n| {
                n.start_position.row as u64 + 1 == owner.start_line
                    && n.start_position.column as u64 == owner.start_column
            })
            .collect();
        if owners.len() != 1 {
            continue;
        }
        let ast_owner = owners[0];
        if !has_decorator(ast_owner, &["Component"]) {
            continue;
        }
        let Some(struct_body) = ast_owner.child_for_field_name("body") else {
            continue;
        };
        let methods = children_of_kind(queries, &owner.id, NodeKind::Method);
        let builds: Vec<_> = methods.iter().filter(|n| n.name == "build").collect();
        if builds.len() != 1 {
            continue;
        }
        let build = builds[0];
        let props: HashSet<_> = struct_body
            .named_children
            .iter()
            .filter(|n| {
                n.node_type() == "public_field_definition"
                    && has_decorator(
                        n,
                        &[
                            "State",
                            "Link",
                            "Prop",
                            "StorageLink",
                            "StorageProp",
                            "LocalStorageLink",
                            "LocalStorageProp",
                            "ObjectLink",
                        ],
                    )
            })
            .filter_map(|n| n.child_for_field_name("name").map(|n| n.text()))
            .collect();
        if props.is_empty() {
            continue;
        }
        let ast_methods: Vec<_> = struct_body
            .named_children
            .iter()
            .filter(|n| n.node_type() == "method_definition")
            .collect();
        let mut added = 0;
        for method in &methods {
            if method.id == build.id || added >= MAX_CALLBACKS_PER_CHANNEL {
                continue;
            }
            let candidates: Vec<_> = ast_methods
                .iter()
                .filter(|n| {
                    n.start_position.row as u64 + 1 == method.start_line
                        && n.start_position.column as u64 == method.start_column
                })
                .collect();
            if candidates.len() != 1 {
                continue;
            }
            let Some(body) = candidates[0].child_for_field_name("body") else {
                continue;
            };
            if !writes(body, &props) {
                continue;
            }
            edges.push(edge(
                &method.id,
                &build.id,
                EdgeKind::Calls,
                Some(method.start_line),
                "arkui-state",
                [
                    ("via", json!("state assignment")),
                    (
                        "registeredAt",
                        json!(format!("{}:{}", build.file_path, build.start_line)),
                    ),
                ],
            ));
            added += 1;
        }
    }
    edges
}
