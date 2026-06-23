//! 引用解析的主调度循环。
//!
//! 这里按“便宜且高置信度优先”的顺序组合内置过滤、import、框架、名称匹配、
//! 函数引用和语言门控；无法立即解析的链式调用会被放到第二阶段处理。

use std::collections::HashMap;

use crate::types::{Language, NodeKind, ReferenceKind, UnresolvedReference};

use super::helpers::{capitalize, chain_language, chain_shape, resolved_by_name};
use super::{ReferenceResolver, ResolutionContext, ResolutionResult, ResolutionStats};
use super::{ResolvedRef, UnresolvedRef};
use crate::resolution::import_resolver::{
    is_php_include_path_ref, resolve_jvm_import, resolve_via_import,
};
use crate::resolution::name_matcher::{
    crosses_known_family, match_function_ref, match_reference, same_language_family,
};

impl<'db> ReferenceResolver<'db> {
    pub fn resolve_all(
        &mut self,
        unresolved_refs: &[UnresolvedReference],
        mut on_progress: Option<&mut dyn FnMut(usize, usize)>,
    ) -> ResolutionResult {
        self.warm_caches();

        // extraction 阶段保存的是轻量 UnresolvedReference；这里补齐 file/language，
        // 让后续策略无需反复按 from_node_id 查上下文。
        let refs = unresolved_refs
            .iter()
            .map(|reference| UnresolvedRef {
                from_node_id: reference.from_node_id.clone(),
                reference_name: reference.reference_name.clone(),
                reference_kind: reference.reference_kind,
                line: reference.line,
                column: reference.column,
                file_path: reference
                    .file_path
                    .clone()
                    .unwrap_or_else(|| self.get_file_path_from_node_id(&reference.from_node_id)),
                language: reference
                    .language
                    .unwrap_or_else(|| self.get_language_from_node_id(&reference.from_node_id)),
                candidates: reference.candidates.clone(),
            })
            .collect::<Vec<_>>();

        let mut resolved = Vec::new();
        let mut unresolved = Vec::new();
        let mut by_method: HashMap<String, u64> = HashMap::new();
        let total = refs.len();
        let mut last_reported_percent = -1i64;

        for (idx, reference) in refs.into_iter().enumerate() {
            if let Some(result) = self.resolve_one(&reference) {
                *by_method
                    .entry(resolved_by_name(result.resolved_by).to_string())
                    .or_default() += 1;
                resolved.push(result);
            } else {
                unresolved.push(reference);
            }

            if let Some(on_progress) = on_progress.as_deref_mut()
                && total > 0
            {
                let current_percent = ((idx * 100) / total) as i64;
                if current_percent > last_reported_percent {
                    last_reported_percent = current_percent;
                    on_progress(idx + 1, total);
                }
            }
        }

        if let Some(on_progress) = on_progress
            && total > 0
        {
            on_progress(total, total);
        }

        ResolutionResult {
            stats: ResolutionStats {
                total: total as u64,
                resolved: resolved.len() as u64,
                unresolved: unresolved.len() as u64,
                by_method,
            },
            resolved,
            unresolved,
        }
    }

    pub(super) fn has_any_possible_match(&mut self, name: &str) -> bool {
        let Some(known_names) = &self.known_names else {
            return true;
        };
        if known_names.contains(name) {
            return true;
        }

        if let Some(dot_idx) = name.find('.') {
            // 这是一个快速“可能性”检查，不做最终解析；只要 receiver、member 或
            // tail 在索引中出现，就保留给更精确策略，减少误判外部符号。
            let receiver = &name[..dot_idx];
            let member = &name[dot_idx + 1..];
            if known_names.contains(receiver) || known_names.contains(member) {
                return true;
            }
            let capitalized = capitalize(receiver);
            if known_names.contains(&capitalized) {
                return true;
            }
            if let Some(last_dot) = name.rfind('.').filter(|idx| *idx > dot_idx) {
                let tail = &name[last_dot + 1..];
                if !tail.is_empty() && known_names.contains(tail) {
                    return true;
                }
            }
        }

        if let Some(colon_idx) = name.find("::") {
            let receiver = &name[..colon_idx];
            let member = &name[colon_idx + 2..];
            if known_names.contains(receiver) || known_names.contains(member) {
                return true;
            }
            if let Some(last_colon) = name.rfind("::").filter(|idx| *idx > colon_idx) {
                let tail = &name[last_colon + 2..];
                if !tail.is_empty() && known_names.contains(tail) {
                    return true;
                }
            }
        }

        if let Some(file_name) = name.rsplit('/').next().filter(|file| *file != name)
            && known_names.contains(file_name)
        {
            return true;
        }

        false
    }

    fn matches_any_import(&mut self, reference: &UnresolvedRef) -> bool {
        self.get_import_mappings(&reference.file_path, reference.language)
            .into_iter()
            .any(|imp| {
                imp.local_name == reference.reference_name
                    || reference
                        .reference_name
                        .starts_with(&format!("{}.", imp.local_name))
                    || reference
                        .reference_name
                        .starts_with(&format!("{}::", imp.local_name))
            })
    }

    pub fn resolve_one(&mut self, reference: &UnresolvedRef) -> Option<ResolvedRef> {
        if self.is_built_in_or_external(reference) {
            return None;
        }

        // 没有任何名字/import/framework 线索时直接跳过，避免每个标准库调用都
        // 扫一遍匹配策略；framework 的 claims_reference 是保留的兜底入口。
        if !self.has_any_possible_match(&reference.reference_name)
            && !self.matches_any_import(reference)
            && !self
                .frameworks
                .iter()
                .any(|framework| framework.claims_reference(&reference.reference_name))
        {
            return None;
        }

        if reference.reference_kind == ReferenceKind::FunctionRef {
            // FunctionRef 表示 extractor 已经知道这是可调用符号引用；优先走
            // import 和函数/方法专用路径，避免被普通变量/类型同名节点抢走。
            if reference.reference_name.starts_with("this.") {
                let result = self.resolve_this_member_fn_ref(reference);
                return self.gate_language(result, reference);
            }
            let via_import_raw = resolve_via_import(reference, self);
            let via_import = self.gate_language(via_import_raw, reference);
            if let Some(via_import) = via_import
                && let Some(target) = self
                    .queries
                    .get_node_by_id(&via_import.target_node_id)
                    .unwrap_or(None)
                && matches!(target.kind, NodeKind::Function | NodeKind::Method)
            {
                return Some(via_import);
            }
            let result = match_function_ref(reference, self);
            return self.gate_language(result, reference);
        }

        if let Some(jvm_import) = resolve_jvm_import(reference, self) {
            return Some(jvm_import);
        }

        if reference.language == Language::Razor
            && let Some(razor_result) = self.resolve_razor_using(reference)
        {
            return Some(razor_result);
        }

        let mut candidates = Vec::new();

        // Framework implementations are task 08; the orchestration slot remains
        // here so shared types and language gating match the TypeScript shape.

        let import_result_raw = resolve_via_import(reference, self);
        if let Some(import_result) = self.gate_language(import_result_raw, reference) {
            if import_result.confidence >= 0.9 {
                return Some(import_result);
            }
            candidates.push(import_result);
        }

        if is_php_include_path_ref(reference) {
            // PHP include/require 的路径引用只应由 import/path 逻辑处理，不能再
            // 退回到同名符号匹配，否则容易把文件路径连到函数或类。
            return candidates
                .into_iter()
                .max_by(|a, b| a.confidence.partial_cmp(&b.confidence).unwrap());
        }

        let name_result_raw = match_reference(reference, self);
        if let Some(name_result) = self.gate_language(name_result_raw, reference) {
            candidates.push(name_result);
        }

        if candidates.is_empty() {
            if reference.reference_kind == ReferenceKind::Calls
                && chain_language(reference.language)
                && chain_shape(&reference.reference_name)
            {
                // `factory().method` 需要 return-type / conformance 边先落库；先记下，
                // 等主解析完成后由 edges.rs 的第二阶段补连。
                self.deferred_chain_refs.push(reference.clone());
            }
            return None;
        }

        candidates
            .into_iter()
            .max_by(|a, b| a.confidence.partial_cmp(&b.confidence).unwrap())
    }

    pub(super) fn gate_language(
        &mut self,
        result: Option<ResolvedRef>,
        reference: &UnresolvedRef,
    ) -> Option<ResolvedRef> {
        let result = result?;
        let target_language = self.get_language_from_node_id(&result.target_node_id);
        if matches!(target_language, Language::Unknown) {
            return Some(result);
        }
        // references/function/type 类边必须在同一语言家族内；imports 可以跨未知语言，
        // 但不能从 web 误连到 JVM/dotnet 等已知且不同的家族。
        if matches!(
            reference.reference_kind,
            ReferenceKind::References
                | ReferenceKind::FunctionRef
                | ReferenceKind::TypeOf
                | ReferenceKind::Returns
                | ReferenceKind::Extends
                | ReferenceKind::Implements
        ) && !same_language_family(target_language, reference.language)
        {
            return None;
        }
        if reference.reference_kind == ReferenceKind::Imports
            && crosses_known_family(target_language, reference.language)
        {
            return None;
        }
        Some(result)
    }

    #[allow(dead_code)]
    pub(super) fn gate_framework_language(
        &mut self,
        result: Option<ResolvedRef>,
        reference: &UnresolvedRef,
    ) -> Option<ResolvedRef> {
        let result = result?;
        if !matches!(
            reference.reference_kind,
            ReferenceKind::References | ReferenceKind::Imports
        ) {
            return Some(result);
        }
        let target_language = self.get_language_from_node_id(&result.target_node_id);
        if target_language != Language::Unknown
            && crosses_known_family(target_language, reference.language)
        {
            None
        } else {
            Some(result)
        }
    }
}
