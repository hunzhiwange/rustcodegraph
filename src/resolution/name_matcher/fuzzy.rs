//! 低置信度的大小写无关兜底匹配。
//!
//! 只有最终候选唯一时才返回 fuzzy 结果；这条路径故意保守，避免一个常见短名在
//! 大仓库里产生错误边。

use std::collections::HashSet;

use crate::resolution::types::{ResolutionContext, ResolvedBy, ResolvedRef, UnresolvedRef};
use crate::types::NodeKind;

use super::common::{apply_language_gate, resolved};

pub fn match_fuzzy(
    reference: &UnresolvedRef,
    context: &mut dyn ResolutionContext,
) -> Option<ResolvedRef> {
    let lower_name = reference.reference_name.to_ascii_lowercase();
    let callable_kinds = HashSet::from([NodeKind::Function, NodeKind::Method, NodeKind::Class]);
    let callable = context
        .get_nodes_by_lower_name(&lower_name)
        .into_iter()
        .filter(|node| callable_kinds.contains(&node.kind))
        .collect::<Vec<_>>();
    let callable = apply_language_gate(callable, reference);
    let same_language = callable
        .iter()
        .filter(|node| node.language == reference.language)
        .cloned()
        .collect::<Vec<_>>();
    let final_candidates = if same_language.is_empty() {
        callable
    } else {
        same_language
    };
    // fuzzy 是最后一道兜底，只在唯一候选时建边；跨语言候选即便唯一也降低置信度。
    if final_candidates.len() == 1 {
        return Some(resolved(
            reference,
            &final_candidates[0].id,
            if final_candidates[0].language != reference.language {
                0.3
            } else {
                0.5
            },
            ResolvedBy::Fuzzy,
        ));
    }
    None
}
