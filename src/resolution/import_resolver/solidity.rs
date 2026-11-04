//! Solidity bindings are read from native import ASTs and restricted to indexed files.
use super::common::resolved;
use super::path::resolve_import_path;
use crate::extraction::grammars::get_parser;
use crate::resolution::types::{ResolutionContext, ResolvedBy, ResolvedRef, UnresolvedRef};
use crate::types::{Language, NodeKind, ReferenceKind};

pub fn resolve_solidity_reference(
    reference: &UnresolvedRef,
    context: &mut dyn ResolutionContext,
) -> Option<ResolvedRef> {
    let content = context.read_file(&reference.file_path)?;
    let mut parser = get_parser(Language::Solidity)?;
    let tree = parser.parse(&content, None)?;
    let name = &reference.reference_name;
    let mut candidates = Vec::new();
    if reference.reference_kind != ReferenceKind::Imports {
        let nodes = context.get_nodes_in_file(&reference.file_path);
        let owner = nodes.iter().find(|n| n.id == reference.from_node_id);
        let scope = owner.and_then(|n| n.qualified_name.rsplit_once("::").map(|(p, _)| p));
        for n in &nodes {
            if n.name == *name
                && (n.kind == NodeKind::Function
                    || matches!(n.kind, NodeKind::Class | NodeKind::Interface)
                    || scope.is_some_and(|s| n.qualified_name == format!("{s}::{name}")))
            {
                candidates.push(n.clone());
            }
        }
    }
    // Inherited bare calls are statically bound through explicit base declarations.
    // Resolve each branch conservatively; ambiguous members remain unresolved.
    if reference.reference_kind == ReferenceKind::Calls
        && !name.contains('.')
        && candidates.is_empty()
    {
        let nodes = context.get_nodes_in_file(&reference.file_path);
        if let Some(owner) = nodes.iter().find(|n| n.id == reference.from_node_id) {
            if let Some((scope, _)) = owner.qualified_name.rsplit_once("::") {
                if let Some(contract) = nodes.iter().find(|n| {
                    n.qualified_name == scope
                        && matches!(n.kind, NodeKind::Class | NodeKind::Interface)
                }) {
                    candidates.extend(inherited_members(
                        contract,
                        name,
                        reference,
                        context,
                        &mut std::collections::HashSet::new(),
                    ));
                }
            }
        }
    }
    for directive in &tree.root_node.named_children {
        if directive.node_type() != "import_directive" || directive.has_error {
            continue;
        }
        let Some(source) = directive.child_for_field_name("source") else {
            continue;
        };
        let path = source.text.trim_matches(['\'', '"']);
        let Some(file) =
            resolve_import_path(path, &reference.file_path, Language::Solidity, context)
        else {
            continue;
        };
        let nodes = context.get_nodes_in_file(&file);
        if reference.reference_kind == ReferenceKind::Imports {
            if name == path {
                candidates.extend(nodes.into_iter().filter(|n| n.kind == NodeKind::File));
            }
            continue;
        }
        // Identifiers and `as` tokens are direct children in this pinned grammar.
        let tokens: Vec<_> = directive
            .children
            .iter()
            .filter(|n| n.node_type() == "identifier" || n.text == "as")
            .map(|n| n.text.as_str())
            .collect();
        let named = directive.children.iter().any(|n| n.text == "{");
        let mut targets = Vec::new();
        if named {
            let mut i = 0;
            while i < tokens.len() {
                let exported = tokens[i];
                let aliased = tokens.get(i + 1) == Some(&"as");
                let local = if aliased {
                    *tokens.get(i + 2)?
                } else {
                    exported
                };
                if name == local {
                    targets.push(exported.to_string());
                }
                if let Some(rest) = name.strip_prefix(&format!("{local}.")) {
                    targets.push(format!("{exported}.{rest}"));
                }
                i += if aliased { 3 } else { 1 };
            }
        } else if let Some(alias) = tokens.iter().rev().find(|v| **v != "as") {
            if let Some(rest) = name.strip_prefix(&format!("{alias}.")) {
                targets.push(rest.to_string());
            }
        } else {
            targets.push(name.clone());
        }
        candidates.extend(nodes.into_iter().filter(|n| {
            let prefix = format!("{file}::");
            let qualified = n
                .qualified_name
                .strip_prefix(&prefix)
                .unwrap_or(&n.qualified_name)
                .replace("::", ".");
            n.kind != NodeKind::File && targets.iter().any(|t| &qualified == t)
        }));
    }
    candidates.sort_by(|a, b| a.id.cmp(&b.id));
    candidates.dedup_by(|a, b| a.id == b.id);
    let [target] = candidates.as_slice() else {
        return None;
    };
    Some(resolved(reference, &target.id, 1.0, ResolvedBy::Import))
}

// No runtime dispatch is inferred here: only members of explicitly resolved bases.
fn inherited_members(
    contract: &crate::types::Node,
    name: &str,
    original: &UnresolvedRef,
    context: &mut dyn ResolutionContext,
    visited: &mut std::collections::HashSet<String>,
) -> Vec<crate::types::Node> {
    if !visited.insert(contract.id.clone()) {
        return Vec::new();
    }
    let Some(content) = context.read_file(&contract.file_path) else {
        return Vec::new();
    };
    let Some(mut parser) = get_parser(Language::Solidity) else {
        return Vec::new();
    };
    let Some(tree) = parser.parse(&content, None) else {
        return Vec::new();
    };
    let Some(declaration) = tree.root_node.named_children.iter().find(|n| {
        matches!(
            n.node_type(),
            "contract_declaration" | "interface_declaration" | "library_declaration"
        ) && n
            .child_for_field_name("name")
            .is_some_and(|n| n.text == contract.name)
    }) else {
        return Vec::new();
    };
    let mut result = Vec::new();
    for base in declaration
        .named_children
        .iter()
        .filter(|n| n.node_type() == "inheritance_specifier")
    {
        let Some(ancestor) = base.child_for_field_name("ancestor") else {
            continue;
        };
        let mut reference = original.clone();
        reference.from_node_id = contract.id.clone();
        reference.file_path = contract.file_path.clone();
        reference.reference_name = ancestor.text.clone();
        reference.reference_kind = ReferenceKind::Extends;
        let Some(resolved) = resolve_solidity_reference(&reference, context) else {
            continue;
        };
        let Some(parent) = context.get_node_by_id(&resolved.target_node_id) else {
            continue;
        };
        let members: Vec<_> = context
            .get_nodes_in_file(&parent.file_path)
            .into_iter()
            .filter(|n| {
                n.kind == NodeKind::Method
                    && n.qualified_name == format!("{}::{name}", parent.qualified_name)
            })
            .collect();
        if members.is_empty() {
            result.extend(inherited_members(&parent, name, original, context, visited));
        } else {
            // Private and external functions cannot be called through a bare
            // inherited identifier. Inspect AST visibility because these nodes
            // do not yet carry extracted visibility metadata.
            if let Some(source) = context.read_file(&parent.file_path) {
                if let Some(tree) = parser.parse(&source, None) {
                    for member in members {
                        let accessible = tree
                            .root_node
                            .named_children
                            .iter()
                            .filter_map(|n| n.child_for_field_name("body"))
                            .flat_map(|body| &body.named_children)
                            .find(|n| {
                                (n.start_position.row + 1) as u64 == member.start_line
                                    && n.start_position.column as u64 == member.start_column
                            })
                            .is_some_and(|n| {
                                !n.children.iter().any(|child| {
                                    matches!(child.text.as_str(), "private" | "external")
                                })
                            });
                        if accessible {
                            result.push(member);
                        }
                    }
                }
            }
        }
    }
    result
}
