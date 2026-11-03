//! Static Nix imports resolve only to their project file, never a similar name.
use crate::resolution::types::{ResolutionContext, ResolvedBy, ResolvedRef, UnresolvedRef};
use crate::types::{NodeKind, ReferenceKind};

use super::common::resolved;
use super::path::resolve_import_path;

pub fn resolve_nix_import(
    reference: &UnresolvedRef,
    context: &mut dyn ResolutionContext,
) -> Option<ResolvedRef> {
    let path = &reference.reference_name;
    if reference.reference_kind != ReferenceKind::Imports
        || !(path.starts_with("./") || path.starts_with("../"))
        || path
            .chars()
            .any(|c| c.is_whitespace() || "{}()[];\"'<>$".contains(c))
    {
        return None;
    }
    let path = resolve_import_path(path, &reference.file_path, reference.language, context)?;
    let target = context
        .get_nodes_in_file(&path)
        .into_iter()
        .find(|node| node.kind == NodeKind::File && node.file_path == path)?;
    Some(resolved(reference, &target.id, 0.9, ResolvedBy::Import))
}
