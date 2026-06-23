//! 将解析结果转换成图边并写回数据库。
//!
//! 这一层只处理 resolver 的副作用：边类型修正、metadata 标注、批量删除已处理
//! unresolved references，以及延迟链式调用的第二阶段解析。

use std::collections::HashMap;

use serde_json::json;

use crate::db::queries::ResolvedReferenceKey;
use crate::types::{Edge, EdgeKind, NodeKind, ReferenceKind, UnresolvedReference};

use super::helpers::{
    edge_kind_from_reference, reference_kind_name, resolved_by_name, scoped_chain_language,
};
use super::{ReferenceResolver, ResolutionResult, ResolutionStats, ResolvedRef};
use crate::resolution::name_matcher::{match_dotted_call_chain, match_scoped_call_chain};

impl<'db> ReferenceResolver<'db> {
    /// 把 `ResolvedRef` 映射成数据库中的 `Edge`。
    ///
    /// 部分引用种类要根据目标节点再归一化：例如调用类/结构体更像实例化，
    /// 普通类继承接口/协议应落为 implements，方便后续图遍历语义一致。
    pub fn create_edges(&mut self, resolved: &[ResolvedRef]) -> Vec<Edge> {
        resolved
            .iter()
            .map(|reference| {
                let mut kind = if reference.original.reference_kind == ReferenceKind::FunctionRef {
                    EdgeKind::References
                } else {
                    edge_kind_from_reference(reference.original.reference_kind)
                };

                if kind == EdgeKind::Extends
                    && let Some(target) = self
                        .queries
                        .get_node_by_id(&reference.target_node_id)
                        .unwrap_or(None)
                    && matches!(target.kind, NodeKind::Interface | NodeKind::Protocol)
                    && let Some(source) = self
                        .queries
                        .get_node_by_id(&reference.original.from_node_id)
                        .unwrap_or(None)
                    && !matches!(source.kind, NodeKind::Interface | NodeKind::Protocol)
                {
                    kind = EdgeKind::Implements;
                }

                // `new Foo()` 或 `Foo(...)` 在抽取阶段常先标成 calls；目标实际是
                // 类型节点时转成 instantiates，避免 callers/callees 混入构造语义。
                if kind == EdgeKind::Calls
                    && let Some(target) = self
                        .queries
                        .get_node_by_id(&reference.target_node_id)
                        .unwrap_or(None)
                    && matches!(target.kind, NodeKind::Class | NodeKind::Struct)
                {
                    kind = EdgeKind::Instantiates;
                }

                let mut metadata = HashMap::from([
                    ("confidence".to_string(), json!(reference.confidence)),
                    (
                        "resolvedBy".to_string(),
                        json!(resolved_by_name(reference.resolved_by)),
                    ),
                ]);
                if reference.original.reference_kind == ReferenceKind::FunctionRef {
                    metadata.insert("fnRef".to_string(), json!(true));
                }

                Edge {
                    source: reference.original.from_node_id.clone(),
                    target: reference.target_node_id.clone(),
                    kind,
                    metadata: Some(metadata),
                    line: Some(reference.original.line),
                    column: Some(reference.original.column),
                    provenance: None,
                }
            })
            .collect()
    }

    pub fn resolve_and_persist(
        &mut self,
        unresolved_refs: &[UnresolvedReference],
        on_progress: Option<&mut dyn FnMut(usize, usize)>,
    ) -> ResolutionResult {
        let result = self.resolve_all(unresolved_refs, on_progress);
        let edges = self.create_edges(&result.resolved);
        if !edges.is_empty() {
            let _ = self.queries.insert_edges(&edges);
        }
        if !result.resolved.is_empty() {
            let keys = result
                .resolved
                .iter()
                .map(|reference| ResolvedReferenceKey {
                    from_node_id: reference.original.from_node_id.clone(),
                    reference_name: reference.original.reference_name.clone(),
                    reference_kind: reference_kind_name(reference.original.reference_kind)
                        .to_string(),
                })
                .collect::<Vec<_>>();
            let _ = self.queries.delete_specific_resolved_references(&keys);
        }
        result
    }

    pub fn resolve_chained_calls_via_conformance(&mut self) -> usize {
        let deferred = std::mem::take(&mut self.deferred_chain_refs);
        if deferred.is_empty() {
            return 0;
        }
        self.clear_caches();
        let mut resolved = Vec::new();
        for reference in deferred {
            // 链式调用需要先写入普通 extends/implements/returns 边，再借这些边
            // 反推 `factory().method` 的接收者类型，因此放在主解析之后第二遍做。
            let chain_match = if scoped_chain_language(reference.language) {
                match_scoped_call_chain(&reference, self)
            } else {
                match_dotted_call_chain(&reference, self)
            };
            if let Some(result) = self.gate_language(chain_match, &reference) {
                resolved.push(result);
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

    pub fn resolve_and_persist_batched(
        &mut self,
        mut on_progress: Option<&mut dyn FnMut(usize, usize)>,
        batch_size: usize,
    ) -> ResolutionResult {
        self.warm_caches();
        let total = self
            .queries
            .get_unresolved_references_count()
            .unwrap_or_default()
            .max(0) as usize;
        let mut processed = 0usize;
        let mut aggregate_stats = ResolutionStats::default();
        let mut prev_remaining = i64::MAX;

        loop {
            let batch = self
                .queries
                .get_unresolved_references_batch(0, batch_size as i64)
                .unwrap_or_default();
            if batch.is_empty() {
                break;
            }

            let result = self.resolve_all(&batch, None);
            let edges = self.create_edges(&result.resolved);
            if !edges.is_empty() {
                let _ = self.queries.insert_edges(&edges);
            }
            if !result.resolved.is_empty() {
                let keys = result
                    .resolved
                    .iter()
                    .map(|reference| ResolvedReferenceKey {
                        from_node_id: reference.original.from_node_id.clone(),
                        reference_name: reference.original.reference_name.clone(),
                        reference_kind: reference_kind_name(reference.original.reference_kind)
                            .to_string(),
                    })
                    .collect::<Vec<_>>();
                let _ = self.queries.delete_specific_resolved_references(&keys);
            }
            if !result.unresolved.is_empty() {
                let keys = result
                    .unresolved
                    .iter()
                    .map(|reference| ResolvedReferenceKey {
                        from_node_id: reference.from_node_id.clone(),
                        reference_name: reference.reference_name.clone(),
                        reference_kind: reference_kind_name(reference.reference_kind).to_string(),
                    })
                    .collect::<Vec<_>>();
                let _ = self.queries.delete_specific_resolved_references(&keys);
            }

            aggregate_stats.total += result.stats.total;
            aggregate_stats.resolved += result.stats.resolved;
            aggregate_stats.unresolved += result.stats.unresolved;
            for (method, count) in result.stats.by_method {
                *aggregate_stats.by_method.entry(method).or_default() += count;
            }

            processed += batch.len();
            if let Some(on_progress) = on_progress.as_deref_mut() {
                on_progress(processed, total);
            }

            if result.resolved.is_empty() && result.unresolved.len() == batch.len() {
                // 当前 batch 全部失败时继续循环只会重复处理同一批 unresolved；
                // 及时退出可避免大仓库进入无收益的解析自旋。
                break;
            }
            let remaining = self
                .queries
                .get_unresolved_references_count()
                .unwrap_or_default();
            if remaining >= prev_remaining {
                // 删除已处理引用后 remaining 应单调下降；未下降说明没有继续推进，
                // 防御性退出，避免数据异常或重复键造成死循环。
                break;
            }
            prev_remaining = remaining;
        }

        aggregate_stats
            .by_method
            .insert("callback-synthesis".to_string(), 0);

        ResolutionResult {
            resolved: Vec::new(),
            unresolved: Vec::new(),
            stats: aggregate_stats,
        }
    }
}
