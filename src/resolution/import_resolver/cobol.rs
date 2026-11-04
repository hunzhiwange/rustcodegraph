//! Conservative COBOL namespaces: COPY files, CALL programs, PERFORM paragraphs.
use super::common::{normalize_path_buf, resolved};
use crate::extraction::{grammars::get_parser, languages::index::extractor_for};
use crate::resolution::types::{ResolutionContext, ResolvedBy, ResolvedRef, UnresolvedRef};
use crate::types::{Language, NodeKind, ReferenceKind};
use crate::web_tree_sitter::SyntaxNode;
use std::path::Path;

pub fn resolve_cobol_reference(
    r: &UnresolvedRef,
    ctx: &mut dyn ResolutionContext,
) -> Option<ResolvedRef> {
    let name = &r.reference_name;
    let mut candidates = if r.reference_kind == ReferenceKind::Imports {
        let dir = Path::new(&r.file_path).parent()?;
        let relative = normalize_path_buf(dir.join(name))
            .to_string_lossy()
            .replace('\\', "/");
        let explicit = name.contains('/') || name.contains('\\');
        let mut files: Vec<_> = ctx
            .get_nodes_by_kind(NodeKind::File)
            .into_iter()
            .filter(|n| {
                if n.language != Language::Cobol {
                    return false;
                }
                let path = Path::new(&n.file_path);
                if explicit {
                    n.file_path.eq_ignore_ascii_case(&relative)
                        || path
                            .with_extension("")
                            .to_string_lossy()
                            .eq_ignore_ascii_case(&relative)
                } else {
                    path.file_stem()
                        .is_some_and(|s| s.to_string_lossy().eq_ignore_ascii_case(name))
                        || path
                            .file_name()
                            .is_some_and(|s| s.to_string_lossy().eq_ignore_ascii_case(name))
                }
            })
            .collect();
        if files
            .iter()
            .any(|n| Path::new(&n.file_path).parent() == Some(dir))
        {
            files.retain(|n| Path::new(&n.file_path).parent() == Some(dir));
        }
        files
    } else if r.reference_kind == ReferenceKind::Calls {
        let source = ctx.read_file(&r.file_path)?;
        let source = extractor_for("cobol")?.pre_parse(&source).unwrap_or(source);
        let tree = get_parser(Language::Cobol)?.parse(&source, None)?;
        let kind = target_kind(&tree.root_node, r.line, r.column)?;
        if kind == NodeKind::Module {
            ctx.get_nodes_by_lower_name(&name.to_ascii_lowercase())
                .into_iter()
                .filter(|n| n.language == Language::Cobol && n.kind == kind)
                .collect()
        } else {
            let nodes = ctx.get_nodes_in_file(&r.file_path);
            let program = nodes
                .iter()
                .filter(|n| {
                    n.kind == NodeKind::Module && n.start_line <= r.line && n.end_line >= r.line
                })
                .min_by_key(|n| n.end_line - n.start_line);
            nodes
                .iter()
                .filter(|n| {
                    n.kind == NodeKind::Function
                        && n.language == Language::Cobol
                        && n.name.eq_ignore_ascii_case(name)
                        && program.is_none_or(|p| {
                            n.start_line >= p.start_line && n.end_line <= p.end_line
                        })
                })
                .cloned()
                .collect()
        }
    } else {
        return None;
    };
    candidates.sort_by(|a, b| a.id.cmp(&b.id));
    candidates.dedup_by(|a, b| a.id == b.id);
    let [target] = candidates.as_slice() else {
        return None;
    };
    Some(resolved(r, &target.id, 1.0, ResolvedBy::Import))
}

// AST target shape distinguishes literal CALL from PERFORM, even across lines.
fn target_kind(node: &SyntaxNode, line: u64, column: u64) -> Option<NodeKind> {
    if (node.start_position.row + 1) as u64 == line && node.start_position.column as u64 == column {
        match node.node_type() {
            "string" => return Some(NodeKind::Module),
            "label" => return Some(NodeKind::Function),
            _ => {}
        }
    }
    node.named_children
        .iter()
        .find_map(|child| target_kind(child, line, column))
}
