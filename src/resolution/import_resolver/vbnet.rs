//! VB imports bind namespaces/types, never filesystem paths or fuzzy names.
use super::common::resolved;
use crate::extraction::grammars::get_parser;
use crate::resolution::types::{ResolutionContext, ResolvedBy, ResolvedRef, UnresolvedRef};
use crate::types::{Language, Node, NodeKind, ReferenceKind};

fn name(node: &Node) -> String {
    node.qualified_name
        .split_once("::")
        .map_or(node.qualified_name.as_str(), |(_, tail)| tail)
        .replace("::", ".")
        .to_lowercase()
}
fn unique(reference: &UnresolvedRef, mut nodes: Vec<Node>) -> Option<ResolvedRef> {
    nodes.sort_by(|a, b| a.id.cmp(&b.id));
    nodes.dedup_by(|a, b| a.id == b.id);
    let [target] = nodes.as_slice() else {
        return None;
    };
    Some(resolved(reference, &target.id, 1.0, ResolvedBy::Import))
}
pub fn resolve_vbnet_reference(
    reference: &UnresolvedRef,
    context: &mut dyn ResolutionContext,
) -> Option<ResolvedRef> {
    let raw = reference.reference_name.to_lowercase();
    let query = raw.strip_prefix("global.").unwrap_or(&raw);
    let leaf = query.rsplit('.').next()?;
    let mut named = context.get_nodes_by_lower_name(leaf);
    if query != leaf {
        named.extend(context.get_nodes_by_lower_name(query));
    }
    let candidates: Vec<_> = named
        .into_iter()
        .filter(|n| {
            n.language == Language::Vbnet
                && match reference.reference_kind {
                    ReferenceKind::Imports => {
                        matches!(n.kind, NodeKind::Namespace | NodeKind::Class)
                    }
                    ReferenceKind::Calls => n.kind == NodeKind::Method,
                    ReferenceKind::Extends | ReferenceKind::Implements => {
                        matches!(n.kind, NodeKind::Class | NodeKind::Interface)
                    }
                    _ => n.kind != NodeKind::File && n.kind != NodeKind::Import,
                }
        })
        .collect();
    // Qualified and Global names must match the entire declaration path.
    let exact: Vec<_> = candidates
        .iter()
        .filter(|n| name(n) == query)
        .cloned()
        .collect();
    if !exact.is_empty()
        || reference.reference_kind == ReferenceKind::Imports
        || raw.starts_with("global.")
    {
        return unique(reference, exact);
    }
    let locals = context.get_nodes_in_file(&reference.file_path);
    if let Some(owner) = locals.iter().find(|n| n.id == reference.from_node_id) {
        let mut scope = name(owner);
        while let Some((parent, _)) = scope.rsplit_once('.') {
            let full = format!("{parent}.{}", query.strip_prefix("me.").unwrap_or(query));
            let found: Vec<_> = candidates
                .iter()
                .filter(|n| name(n) == full)
                .cloned()
                .collect();
            if !found.is_empty() {
                return unique(reference, found);
            }
            scope = parent.to_owned();
        }
    }
    let source = context.read_file(&reference.file_path)?;
    let mut parser = get_parser(Language::Vbnet)?;
    let source = format!("{source}\n");
    let tree = parser.parse(&source, None)?;
    let mut paths = Vec::new();
    for import in &tree.root_node.named_children {
        if import.node_type() != "imports_statement" || import.has_error {
            continue;
        }
        let Some(namespace) = import.child_for_field_name("namespace") else {
            continue;
        };
        let target = namespace.text.to_lowercase();
        let target = target.strip_prefix("global.").unwrap_or(&target);
        if let Some(alias) = import.child_for_field_name("alias") {
            let alias = alias.text.to_lowercase();
            if query == alias {
                paths.push(target.to_owned());
            }
            if let Some(rest) = query.strip_prefix(&format!("{alias}.")) {
                paths.push(format!("{target}.{rest}"));
            }
        } else {
            paths.push(format!("{target}.{query}"));
        }
    }
    unique(
        reference,
        candidates
            .into_iter()
            .filter(|n| paths.contains(&name(n)))
            .collect(),
    )
}
