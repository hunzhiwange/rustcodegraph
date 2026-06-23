//! Export and re-export symbol resolution.
//!
//! JS/TS barrel 文件会把真正定义藏在多层 re-export 后面。这里沿 re-export 链
//! 找到最终导出符号，并为 namespace/static member 访问做二次定位。

use std::collections::HashSet;

use crate::resolution::types::{ReExport, ResolutionContext, UnresolvedRef};
use crate::types::{Language, Node, NodeKind, ReferenceKind};

use super::path::resolve_import_path;

const REEXPORT_MAX_DEPTH: usize = 8;

#[derive(Debug, Clone)]
pub(super) struct SymbolWant {
    pub(super) is_default: bool,
    pub(super) is_namespace: bool,
    pub(super) exported_name: String,
    pub(super) member_name: Option<String>,
}

pub(super) fn find_exported_symbol(
    file_path: &str,
    want: SymbolWant,
    language: Language,
    context: &mut dyn ResolutionContext,
    visited: &mut HashSet<String>,
    depth: usize,
) -> Option<Node> {
    // 限制深度并记录 visited，避免 barrel 循环让单次解析无限递归。
    if depth > REEXPORT_MAX_DEPTH || !visited.insert(file_path.to_string()) {
        return None;
    }
    let nodes_in_file = context.get_nodes_in_file(file_path);
    if want.is_default {
        if let Some(direct) = nodes_in_file
            .iter()
            .find(|node| node.is_exported.unwrap_or(false) && node.kind == NodeKind::Component)
            .or_else(|| {
                nodes_in_file.iter().find(|node| {
                    node.is_exported.unwrap_or(false)
                        && matches!(node.kind, NodeKind::Function | NodeKind::Class)
                })
            })
        {
            return Some(direct.clone());
        }
    } else if want.is_namespace {
        if let Some(member_name) = &want.member_name
            && let Some(direct) = nodes_in_file
                .iter()
                .find(|node| node.name == *member_name && node.is_exported.unwrap_or(false))
        {
            return Some(direct.clone());
        }
    } else if let Some(direct) = nodes_in_file
        .iter()
        .find(|node| node.name == want.exported_name && node.is_exported.unwrap_or(false))
    {
        return Some(direct.clone());
    }

    let re_exports = context.get_re_exports(file_path, language);
    if re_exports.is_empty() {
        return None;
    }
    let target_name = if want.is_default {
        "default"
    } else {
        &want.exported_name
    };
    for re_export in &re_exports {
        if let ReExport::Named {
            exported_name,
            original_name,
            source,
        } = re_export
        {
            if exported_name != target_name {
                continue;
            }
            let Some(next) = resolve_import_path(source, file_path, language, context) else {
                continue;
            };
            let chained = find_exported_symbol(
                &next,
                SymbolWant {
                    is_default: original_name == "default",
                    is_namespace: false,
                    exported_name: original_name.clone(),
                    member_name: None,
                },
                language,
                context,
                visited,
                depth + 1,
            );
            if chained.is_some() {
                return chained;
            }
        }
    }
    for re_export in &re_exports {
        if let ReExport::Wildcard { source } = re_export {
            let Some(next) = resolve_import_path(source, file_path, language, context) else {
                continue;
            };
            if let Some(chained) =
                find_exported_symbol(&next, want.clone(), language, context, visited, depth + 1)
            {
                return Some(chained);
            }
        }
    }
    None
}

pub(super) fn resolve_static_member(
    container: &Node,
    reference: &UnresolvedRef,
    local_name: &str,
    context: &mut dyn ResolutionContext,
) -> Option<Node> {
    // `Foo.bar` 在 import 层先解析到 Foo，再在同文件同 qualified_name 下找成员；
    // call 引用优先 method/function，属性访问则接受第一个成员节点。
    if !matches!(
        container.kind,
        NodeKind::Class
            | NodeKind::Struct
            | NodeKind::Interface
            | NodeKind::Enum
            | NodeKind::Trait
            | NodeKind::Protocol
    ) {
        return None;
    }
    let member = reference.reference_name[local_name.len() + 1..]
        .split('.')
        .next()?;
    let candidates = context
        .get_nodes_by_qualified_name(&format!("{}::{member}", container.qualified_name))
        .into_iter()
        .filter(|node| node.file_path == container.file_path)
        .collect::<Vec<_>>();
    if candidates.is_empty() {
        return None;
    }
    if reference.reference_kind == ReferenceKind::Calls
        && let Some(callable) = candidates
            .iter()
            .find(|node| matches!(node.kind, NodeKind::Method | NodeKind::Function))
    {
        return Some(callable.clone());
    }
    Some(candidates[0].clone())
}
