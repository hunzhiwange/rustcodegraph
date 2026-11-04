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
