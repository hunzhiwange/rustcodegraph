//! 精确名称和限定名匹配。
//!
//! 当多个节点同名时，这里用路径接近度、语言、引用种类和导出状态打分，尽量在
//! 不读取源码的情况下选出最可能目标。

use crate::resolution::types::{ResolutionContext, ResolvedBy, ResolvedRef, UnresolvedRef};
use crate::types::{Node, NodeKind, ReferenceKind};

use super::common::{apply_language_gate, compute_path_proximity, resolved};

pub fn match_by_exact_name(
    reference: &UnresolvedRef,
    context: &mut dyn ResolutionContext,
) -> Option<ResolvedRef> {
    let candidates = apply_language_gate(
        context.get_nodes_by_name(&reference.reference_name),
        reference,
    );
    if candidates.is_empty() {
        return None;
    }
    if candidates.len() == 1 {
        return Some(resolved(
            reference,
            &candidates[0].id,
            if candidates[0].language != reference.language {
                0.5
            } else {
                0.9
            },
            ResolvedBy::ExactMatch,
        ));
    }
    let best = find_best_match(reference, &candidates)?;
    let proximity = compute_path_proximity(&reference.file_path, &best.file_path);
    Some(resolved(
        reference,
        &best.id,
        if proximity >= 30 { 0.7 } else { 0.4 },
        ResolvedBy::ExactMatch,
    ))
}

pub fn match_by_qualified_name(
    reference: &UnresolvedRef,
    context: &mut dyn ResolutionContext,
) -> Option<ResolvedRef> {
    if !reference.reference_name.contains("::") && !reference.reference_name.contains('.') {
        return None;
    }
    let candidates = context.get_nodes_by_qualified_name(&reference.reference_name);
    if candidates.len() == 1 {
        return Some(resolved(
            reference,
            &candidates[0].id,
            0.95,
            ResolvedBy::QualifiedName,
        ));
    }

    let last_name = reference
        .reference_name
        .split([':', '.'])
        .rfind(|part| !part.is_empty())?;
    for candidate in context.get_nodes_by_name(last_name) {
        if candidate
            .qualified_name
            .ends_with(&reference.reference_name)
        {
            return Some(resolved(
                reference,
                &candidate.id,
                0.85,
                ResolvedBy::QualifiedName,
            ));
        }
    }
    None
}

fn find_best_match(reference: &UnresolvedRef, candidates: &[Node]) -> Option<Node> {
    // 同文件和同语言权重最高；路径接近度只是 tie-breaker，避免 monorepo 中同名
    // API 被远处 package 抢走。
    let mut best_score = -1.0;
    let mut best_node = None;

    for candidate in candidates {
        let mut score = 0.0;
        if candidate.file_path == reference.file_path {
            score += 100.0;
        }
        score += compute_path_proximity(&reference.file_path, &candidate.file_path) as f64;
        if candidate.language == reference.language {
            score += 50.0;
        } else {
            score -= 80.0;
        }
        if reference.reference_kind == ReferenceKind::Calls
            && (candidate.kind == NodeKind::Function || candidate.kind == NodeKind::Method)
        {
            score += 25.0;
        }
        if reference.reference_kind == ReferenceKind::Instantiates
            && matches!(
                candidate.kind,
                NodeKind::Class | NodeKind::Struct | NodeKind::Interface
            )
        {
            score += 25.0;
        }
        if reference.reference_kind == ReferenceKind::Decorates {
            if candidate.kind == NodeKind::Function || candidate.kind == NodeKind::Method {
                score += 25.0;
            } else if candidate.kind == NodeKind::Class || candidate.kind == NodeKind::Interface {
                score += 15.0;
            }
        }
        if candidate.is_exported.unwrap_or(false) {
            score += 10.0;
        }
        if candidate.file_path == reference.file_path {
            let distance = candidate.start_line.abs_diff(reference.line) as f64;
            score += (20.0 - distance / 10.0).max(0.0);
        }
        if score > best_score {
            best_score = score;
            best_node = Some(candidate.clone());
        }
    }

    best_node
}
