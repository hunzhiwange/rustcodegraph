//! Terraform addresses are scoped to a directory, never a global name search.
use std::path::Path;

use crate::extraction::index::extract_from_source;
use crate::resolution::types::{ResolutionContext, ResolvedBy, ResolvedRef, UnresolvedRef};
use crate::types::{Language, NodeKind};

use super::common::{normalize_path_buf, path_relative_to_project, resolved};

pub fn resolve_terraform_reference(
    reference: &UnresolvedRef,
    context: &mut dyn ResolutionContext,
) -> Option<ResolvedRef> {
    let dir = Path::new(&reference.file_path).parent()?;
    let mut target_dir = dir.to_string_lossy().replace('\\', "/");
    let mut address = reference.reference_name.as_str();
    let mut file_target = false;
    if let Some((module, scoped)) = address.split_once(':') {
        if !module.starts_with("module.") {
            return None;
        }
        let imports = context
            .get_nodes_by_qualified_name(module)
            .into_iter()
            .filter(|n| {
                n.language == Language::Terraform && Path::new(&n.file_path).parent() == Some(dir)
            })
            .collect::<Vec<_>>();
        let [module_node] = imports.as_slice() else {
            return None;
        };
        let content = context.read_file(&module_node.file_path)?;
        let extraction = extract_from_source(&module_node.file_path, &content, None, None);
        let prefix = format!("{module}:file:");
        let source = extraction
            .unresolved_references
            .iter()
            .find(|r| r.from_node_id == module_node.id && r.reference_name.starts_with(&prefix))?
            .reference_name
            .strip_prefix(&prefix)?;
        let root = context.get_project_root();
        let absolute = normalize_path_buf(Path::new(&root).join(dir).join(source));
        target_dir = path_relative_to_project(&root, absolute)?;
        if scoped.starts_with("file:") {
            file_target = true;
        } else if scoped.starts_with("var.") || scoped.starts_with("output.") {
            address = scoped;
        } else {
            return None;
        }
    }
    let mut candidates = Vec::new();
    if file_target {
        let mut files = context.get_all_files();
        files.sort();
        // A Terraform module consists of all configuration files in one directory.
        // One deterministic file anchors the imports edge; input/output edges
        // resolve independently across all of those files.
        for file in files {
            if Path::new(&file).parent() != Some(Path::new(&target_dir))
                || !matches!(
                    Path::new(&file).extension().and_then(|s| s.to_str()),
                    Some("tf" | "tofu")
                )
            {
                continue;
            }
            if let Some(node) = context
                .get_nodes_in_file(&file)
                .into_iter()
                .find(|n| n.language == Language::Terraform && n.kind == NodeKind::File)
            {
                return Some(resolved(reference, &node.id, 1.0, ResolvedBy::Import));
            }
        }
        return None;
    }
    for node in context.get_nodes_by_qualified_name(address) {
        if node.language == Language::Terraform
            && Path::new(&node.file_path).parent() == Some(Path::new(&target_dir))
        {
            candidates.push(node);
        }
    }
    let [target] = candidates.as_slice() else {
        return None;
    };
    Some(resolved(
        reference,
        &target.id,
        1.0,
        ResolvedBy::QualifiedName,
    ))
}
