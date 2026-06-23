//! 函数引用专用匹配。
//!
//! FunctionRef 来自 extractor 的高信号引用类型，通常表示“这里引用的是可调用体”，
//! 因此候选会限制在 function/method，并额外处理 Swift 方法歧义。

use crate::resolution::types::{ResolutionContext, ResolvedBy, ResolvedRef, UnresolvedRef};
use crate::types::{Language, NodeKind};

use super::common::{resolved, same_language_family};

pub fn match_function_ref(
    reference: &UnresolvedRef,
    context: &mut dyn ResolutionContext,
) -> Option<ResolvedRef> {
    if reference.reference_name.starts_with("this.") {
        return None;
    }

    let bare_fn_only = matches!(
        reference.language,
        Language::TypeScript
            | Language::Tsx
            | Language::JavaScript
            | Language::Jsx
            | Language::Cpp
            | Language::Python
            | Language::Php
    );

    // JS/Python/PHP/C++ 的裸函数名通常不应匹配任意类方法；Swift/Kotlin 等语言则
    // 可能在同一类型作用域内裸写方法名，所以保留 method 候选。
    if reference.reference_name.contains("::") {
        let member_name = reference.reference_name.rsplit("::").next()?;
        let mut scoped = context
            .get_nodes_by_name(member_name)
            .into_iter()
            .filter(|node| {
                (node.kind == NodeKind::Function || node.kind == NodeKind::Method)
                    && same_language_family(node.language, reference.language)
                    && node.id != reference.from_node_id
                    && (node.qualified_name == reference.reference_name
                        || node
                            .qualified_name
                            .ends_with(&format!("::{}", reference.reference_name)))
            })
            .collect::<Vec<_>>();
        if scoped.is_empty() {
            return None;
        }
        let same_file = scoped
            .iter()
            .filter(|node| node.file_path == reference.file_path)
            .cloned()
            .collect::<Vec<_>>();
        if same_file.is_empty() && scoped.len() > 1 {
            return None;
        }
        if !same_file.is_empty() {
            scoped = same_file;
        }
        scoped.sort_by_key(|node| node.start_line);
        return Some(resolved(
            reference,
            &scoped[0].id,
            0.9,
            ResolvedBy::FunctionRef,
        ));
    }

    let mut candidates = context
        .get_nodes_by_name(&reference.reference_name)
        .into_iter()
        .filter(|node| {
            (node.kind == NodeKind::Function || (!bare_fn_only && node.kind == NodeKind::Method))
                && same_language_family(node.language, reference.language)
                && node.id != reference.from_node_id
        })
        .collect::<Vec<_>>();
    if candidates.is_empty() {
        return None;
    }

    if reference.language == Language::Swift
        && candidates.iter().any(|node| node.kind == NodeKind::Method)
    {
        // Swift 同文件里常有多个同名方法；只有能落到当前类型前缀时才解析，
        // 否则返回 None 让调用方避免建立低置信度边。
        let from_node = context.get_node_by_id(&reference.from_node_id);
        let class_prefix = from_node.and_then(|node| {
            node.qualified_name
                .rfind("::")
                .filter(|idx| *idx > 0)
                .map(|idx| node.qualified_name[..idx].to_string())
        });
        candidates.retain(|node| {
            if node.kind != NodeKind::Method {
                return true;
            }
            let Some(class_prefix) = &class_prefix else {
                return false;
            };
            let Some(sep) = node.qualified_name.rfind("::") else {
                return false;
            };
            let method_prefix = &node.qualified_name[..sep];
            method_prefix == class_prefix
                || method_prefix.ends_with(&format!("::{class_prefix}"))
                || class_prefix.ends_with(&format!("::{method_prefix}"))
        });
        if candidates.is_empty() {
            return None;
        }
    }

    let mut same_file = candidates
        .iter()
        .filter(|node| node.file_path == reference.file_path)
        .cloned()
        .collect::<Vec<_>>();
    if !same_file.is_empty() {
        if reference.language == Language::Swift
            && same_file.len() > 1
            && same_file.iter().all(|node| node.kind == NodeKind::Method)
        {
            // 多个同文件 Swift 方法同名时，行号排序不足以 disambiguate，保持未解析。
            return None;
        }
        same_file.sort_by_key(|node| node.start_line);
        return Some(resolved(
            reference,
            &same_file[0].id,
            if same_file.len() == 1 { 0.95 } else { 0.9 },
            ResolvedBy::FunctionRef,
        ));
    }

    if candidates.len() == 1 {
        return Some(resolved(
            reference,
            &candidates[0].id,
            0.8,
            ResolvedBy::FunctionRef,
        ));
    }

    None
}
