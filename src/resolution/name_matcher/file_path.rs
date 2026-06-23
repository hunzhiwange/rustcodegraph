//! 文件路径形状引用的匹配。
//!
//! include/require、barrel re-export 和部分模板语言会以路径而不是符号名引用目标；
//! 这里优先把这些引用连到 file 节点。

use crate::resolution::types::{ResolutionContext, ResolvedBy, ResolvedRef, UnresolvedRef};
use crate::types::{Node, NodeKind};

use super::common::{compute_path_proximity, resolved, same_language_family};

pub fn match_by_file_path(
    reference: &UnresolvedRef,
    context: &mut dyn ResolutionContext,
) -> Option<ResolvedRef> {
    if !reference.reference_name.contains('/') && !has_short_extension(&reference.reference_name) {
        return None;
    }
    // 只有像路径或带短扩展名的名字才进入该策略，避免普通 dotted 符号被误当文件。
    let file_name = reference.reference_name.rsplit('/').next()?.to_string();
    if file_name.is_empty() {
        return None;
    }

    let file_nodes = context
        .get_nodes_by_name(&file_name)
        .into_iter()
        .filter(|node| node.kind == NodeKind::File)
        .collect::<Vec<_>>();
    if file_nodes.is_empty() {
        return None;
    }

    if let Some(exact) = file_nodes.iter().find(|node| {
        node.qualified_name == reference.reference_name
            || node.file_path == reference.reference_name
    }) {
        return Some(resolved(reference, &exact.id, 0.95, ResolvedBy::FilePath));
    }

    let suffix_matches = file_nodes
        .iter()
        .filter(|node| {
            node.qualified_name.ends_with(&reference.reference_name)
                || node.file_path.ends_with(&reference.reference_name)
        })
        .cloned()
        .collect::<Vec<_>>();
    if !suffix_matches.is_empty() {
        let picked = pick_closest_file_node(&suffix_matches, reference);
        return Some(resolved(reference, &picked.id, 0.85, ResolvedBy::FilePath));
    }

    if file_nodes.len() == 1 {
        return Some(resolved(
            reference,
            &file_nodes[0].id,
            0.7,
            ResolvedBy::FilePath,
        ));
    }

    None
}

fn has_short_extension(value: &str) -> bool {
    let Some((_, ext)) = value.rsplit_once('.') else {
        return false;
    };
    (1..=4).contains(&ext.len())
        && ext
            .chars()
            .next()
            .map(|ch| ch.is_ascii_alphabetic())
            .unwrap_or(false)
        && ext.chars().all(|ch| ch.is_ascii_alphanumeric())
}

fn dir_of(path: &str) -> &str {
    path.rsplit_once('/').map(|(dir, _)| dir).unwrap_or("")
}

fn pick_closest_file_node(candidates: &[Node], reference: &UnresolvedRef) -> Node {
    // 同目录优先，其次看公共路径前缀和语言家族；这能处理 `./foo` 与 monorepo 中
    // 多个 `foo.ts` 同名文件的常见歧义。
    let ref_dir = dir_of(&reference.file_path);
    let same_dir = candidates
        .iter()
        .filter(|node| dir_of(&node.file_path) == ref_dir)
        .collect::<Vec<_>>();
    let pool = if same_dir.is_empty() {
        candidates.iter().collect::<Vec<_>>()
    } else {
        same_dir
    };
    pool.into_iter()
        .max_by_key(|node| {
            compute_path_proximity(&reference.file_path, &node.file_path)
                + if same_language_family(node.language, reference.language) {
                    5
                } else {
                    0
                }
        })
        .cloned()
        .unwrap_or_else(|| candidates[0].clone())
}
