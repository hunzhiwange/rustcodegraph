//! `this.member` 函数引用解析。
//!
//! 第一阶段只解析当前类型内的直接成员；若成员来自父类/接口，则延迟到
//! extends/implements 边写入后再沿继承图查找。

use std::collections::HashSet;

use crate::types::{EdgeKind, NodeKind};

use super::helpers::supertype_bearing;
use super::{ReferenceResolver, ResolutionContext, ResolvedBy, ResolvedRef, UnresolvedRef};
use crate::resolution::name_matcher::same_language_family;

impl<'db> ReferenceResolver<'db> {
    pub(super) fn resolve_this_member_fn_ref(
        &mut self,
        reference: &UnresolvedRef,
    ) -> Option<ResolvedRef> {
        let member = reference.reference_name.strip_prefix("this.")?;
        if member.is_empty() {
            return None;
        }
        let from_node = self
            .queries
            .get_node_by_id(&reference.from_node_id)
            .unwrap_or(None)?;
        let class_prefix =
            if supertype_bearing(from_node.kind) || from_node.kind == NodeKind::Module {
                from_node.qualified_name
            } else {
                let sep = from_node.qualified_name.rfind("::")?;
                if sep == 0 {
                    return None;
                }
                from_node.qualified_name[..sep].to_string()
            };
        let mut candidates = self
            .get_nodes_by_qualified_name(&format!("{class_prefix}::{member}"))
            .into_iter()
            .filter(|node| {
                matches!(node.kind, NodeKind::Function | NodeKind::Method)
                    && node.file_path == reference.file_path
                    && node.id != reference.from_node_id
            })
            .collect::<Vec<_>>();
        if candidates.is_empty() {
            // 父类/接口成员依赖继承边，主解析前可能还不存在；放入 deferred 队列，
            // 由 `resolve_deferred_this_member_refs` 在图结构更完整时重试。
            self.deferred_this_member_refs.push(reference.clone());
            return None;
        }
        candidates.sort_by_key(|node| node.start_line);
        Some(ResolvedRef {
            original: reference.clone(),
            target_node_id: candidates[0].id.clone(),
            confidence: 0.95,
            resolved_by: ResolvedBy::FunctionRef,
        })
    }

    pub fn resolve_deferred_this_member_refs(&mut self) -> usize {
        let deferred = std::mem::take(&mut self.deferred_this_member_refs);
        if deferred.is_empty() {
            return 0;
        }
        self.clear_caches();
        let mut resolved = Vec::new();
        for reference in deferred {
            let Some(member) = reference
                .reference_name
                .strip_prefix("this.")
                .filter(|m| !m.is_empty())
            else {
                continue;
            };
            let Some(from_node) = self
                .queries
                .get_node_by_id(&reference.from_node_id)
                .unwrap_or(None)
            else {
                continue;
            };
            let class_name =
                if supertype_bearing(from_node.kind) || from_node.kind == NodeKind::Module {
                    from_node.name
                } else {
                    let Some(sep) = from_node.qualified_name.rfind("::") else {
                        continue;
                    };
                    let class_prefix = &from_node.qualified_name[..sep];
                    class_prefix
                        .rsplit("::")
                        .next()
                        .unwrap_or(class_prefix)
                        .to_string()
                };
            let mut frontier = self
                .get_nodes_by_name(&class_name)
                .into_iter()
                .filter(|node| {
                    supertype_bearing(node.kind) && node.file_path == reference.file_path
                })
                .collect::<Vec<_>>();
            if frontier.is_empty() {
                frontier = self
                    .get_nodes_by_name(&class_name)
                    .into_iter()
                    .filter(|node| {
                        supertype_bearing(node.kind)
                            && same_language_family(node.language, reference.language)
                    })
                    .collect();
            }
            let mut seen = frontier
                .iter()
                .map(|node| node.id.clone())
                .collect::<HashSet<_>>();
            let mut target = None;
            for _depth in 0..5 {
                // 限深 BFS 足够覆盖常见继承链，同时防止环形继承或框架基类网状结构
                // 把一次 `this.foo` 解析变成全图遍历。
                if frontier.is_empty() || target.is_some() {
                    break;
                }
                let mut next = Vec::new();
                for type_node in frontier {
                    for edge in self
                        .queries
                        .get_outgoing_edges(
                            &type_node.id,
                            Some(vec![EdgeKind::Implements, EdgeKind::Extends]),
                            None,
                        )
                        .unwrap_or_default()
                    {
                        let Some(super_node) =
                            self.queries.get_node_by_id(&edge.target).unwrap_or(None)
                        else {
                            continue;
                        };
                        if !seen.insert(super_node.id.clone())
                            || !supertype_bearing(super_node.kind)
                        {
                            continue;
                        }
                        for contains in self
                            .queries
                            .get_outgoing_edges(
                                &super_node.id,
                                Some(vec![EdgeKind::Contains]),
                                None,
                            )
                            .unwrap_or_default()
                        {
                            let Some(method) = self
                                .queries
                                .get_node_by_id(&contains.target)
                                .unwrap_or(None)
                            else {
                                continue;
                            };
                            if method.name == member
                                && matches!(method.kind, NodeKind::Function | NodeKind::Method)
                                && same_language_family(method.language, reference.language)
                            {
                                target = Some(method);
                                break;
                            }
                        }
                        if target.is_some() {
                            break;
                        }
                        next.push(super_node);
                    }
                    if target.is_some() {
                        break;
                    }
                }
                frontier = next;
            }
            if let Some(target) = target {
                resolved.push(ResolvedRef {
                    original: reference,
                    target_node_id: target.id,
                    confidence: 0.85,
                    resolved_by: ResolvedBy::FunctionRef,
                });
            }
        }
        if resolved.is_empty() {
            return 0;
        }
        let edges = self.create_edges(&resolved);
        if !edges.is_empty() {
            let _ = self.queries.insert_edges(&edges);
            self.clear_caches();
        }
        edges.len()
    }
}
