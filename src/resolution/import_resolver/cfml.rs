//! Static CFML paths only: unknown runtime mappings must not become basename matches.
use super::common::{normalize_path_buf, resolved};
use crate::resolution::types::{ResolutionContext, ResolvedBy, ResolvedRef, UnresolvedRef};
use crate::types::{Language, NodeKind, ReferenceKind};
use std::path::Path;

pub fn resolve_cfml_reference(
    reference: &UnresolvedRef,
    context: &mut dyn ResolutionContext,
) -> Option<ResolvedRef> {
    let dir = Path::new(&reference.file_path).parent()?;
    let raw = reference.reference_name.replace('\\', "/");
    let mut candidates = match reference.reference_kind {
        ReferenceKind::Imports => {
            // Includes resolve from the including file. Absolute paths require
            // application/server mappings, which are deliberately not guessed.
            if raw.starts_with('/') || raw.contains(':') {
                return None;
            }
            let path = normalized(dir.join(&raw));
            context
                .get_nodes_by_kind(NodeKind::File)
                .into_iter()
                .filter(|n| n.language == Language::Cfml && n.file_path.eq_ignore_ascii_case(&path))
                .collect::<Vec<_>>()
        }
        ReferenceKind::Instantiates | ReferenceKind::Extends => {
            let path = format!("{}.cfc", raw.replace('.', "/"));
            let local = normalized(dir.join(&path));
            let nodes: Vec<_> = context
                .get_nodes_by_kind(NodeKind::Class)
                .into_iter()
                .filter(|n| n.language == Language::Cfml)
                .collect();
            let local_nodes: Vec<_> = nodes
                .iter()
                .filter(|n| n.file_path.eq_ignore_ascii_case(&local))
                .cloned()
                .collect();
            if !local_nodes.is_empty() {
                local_nodes
            } else {
                // Root-relative dotted names must match the entire path; never
                // drop package segments to rescue an external component name.
                nodes
                    .into_iter()
                    .filter(|n| n.file_path.eq_ignore_ascii_case(&path))
                    .collect()
            }
        }
        ReferenceKind::Calls => {
            if let Some((component, method)) = raw.split_once("::") {
                // This marker is emitted only for an AST construction receiver.
                let mut constructor = reference.clone();
                constructor.reference_kind = ReferenceKind::Instantiates;
                constructor.reference_name = component.to_owned();
                let owner = resolve_cfml_reference(&constructor, context)?;
                let target = context
                    .get_nodes_by_kind(NodeKind::Class)
                    .into_iter()
                    .find(|n| n.id == owner.target_node_id)?;
                let methods: Vec<_> = context
                    .get_nodes_in_file(&target.file_path)
                    .into_iter()
                    .filter(|n| {
                        n.language == Language::Cfml
                            && n.kind == NodeKind::Method
                            && n.name.eq_ignore_ascii_case(method)
                            && n.qualified_name.eq_ignore_ascii_case(&format!(
                                "{}::{}",
                                target.qualified_name, method
                            ))
                    })
                    .collect();
                return match methods.as_slice() {
                    [method] => Some(resolved(reference, &method.id, 1.0, ResolvedBy::Import)),
                    _ => None,
                };
            }
            let name = if let Some((scope, name)) = raw.split_once('.') {
                if !scope.eq_ignore_ascii_case("this") && !scope.eq_ignore_ascii_case("variables") {
                    return None;
                }
                name
            } else {
                &raw
            };
            context
                .get_nodes_in_file(&reference.file_path)
                .into_iter()
                .filter(|n| {
                    matches!(n.kind, NodeKind::Method | NodeKind::Function)
                        && n.language == Language::Cfml
                        && n.name.eq_ignore_ascii_case(name)
                })
                .collect()
        }
        _ => return None,
    };
    candidates.sort_by(|a, b| a.id.cmp(&b.id));
    candidates.dedup_by(|a, b| a.id == b.id);
    let [target] = candidates.as_slice() else {
        return None;
    };
    Some(resolved(reference, &target.id, 1.0, ResolvedBy::Import))
}

fn normalized(path: std::path::PathBuf) -> String {
    normalize_path_buf(path)
        .to_string_lossy()
        .replace('\\', "/")
}
