//! Python import binding resolution.
//!
//! Python 的模块引用既可能指向文件，也可能指向模块里的成员。这里先根据
//! ImportMapping 还原 module path，再用文件节点或成员节点落点。

use crate::resolution::types::{
    ImportMapping, ResolutionContext, ResolvedBy, ResolvedRef, UnresolvedRef,
};
use crate::types::{Language, Node, NodeKind, ReferenceKind};

use super::common::{ends_with_path, resolved};
use super::path::resolve_import_path;

pub(super) fn resolve_python_module_member(
    reference: &UnresolvedRef,
    imports: &[ImportMapping],
    context: &mut dyn ResolutionContext,
) -> Option<ResolvedRef> {
    // 处理 `import pkg as p; p.member` 和 `from pkg import mod; mod.member`。
    // 解析到同文件时跳过，避免局部变量/模块名自引用造成环。
    let dot_idx = reference.reference_name.find('.')?;
    if dot_idx == 0 {
        return None;
    }
    let receiver = &reference.reference_name[..dot_idx];
    let member = reference.reference_name[dot_idx + 1..].split('.').next()?;
    if member.is_empty() {
        return None;
    }
    for imp in imports.iter().filter(|imp| imp.local_name == receiver) {
        let module_path = if imp.is_namespace {
            imp.source.clone()
        } else if imp.source.ends_with('.') {
            format!("{}{}", imp.source, imp.local_name)
        } else {
            format!("{}.{}", imp.source, imp.local_name)
        };
        let resolved_path = resolve_import_path(
            &module_path,
            &reference.file_path,
            reference.language,
            context,
        )
        .or_else(|| {
            find_python_module_file(&module_path, context, &reference.file_path)
                .map(|n| n.file_path)
        });
        let Some(resolved_path) = resolved_path.filter(|path| path != &reference.file_path) else {
            continue;
        };
        if let Some(target) = context
            .get_nodes_in_file(&resolved_path)
            .into_iter()
            .find(|node| {
                node.name == member
                    && matches!(
                        node.kind,
                        NodeKind::Function
                            | NodeKind::Class
                            | NodeKind::Variable
                            | NodeKind::Constant
                    )
            })
        {
            return Some(resolved(reference, &target.id, 0.85, ResolvedBy::Import));
        }
    }
    None
}

pub(super) fn resolve_module_import_to_file(
    reference: &UnresolvedRef,
    imports: &[ImportMapping],
    context: &mut dyn ResolutionContext,
) -> Option<ResolvedRef> {
    if reference.reference_kind != ReferenceKind::Imports || reference.reference_name.contains('.')
    {
        return None;
    }
    for imp in imports
        .iter()
        .filter(|imp| imp.local_name == reference.reference_name)
    {
        let module_path = if imp.is_namespace || imp.is_default {
            imp.source.clone()
        } else if reference.language == Language::Python {
            if imp.source.ends_with('.') {
                format!("{}{}", imp.source, imp.local_name)
            } else {
                format!("{}.{}", imp.source, imp.local_name)
            }
        } else {
            continue;
        };
        if let Some(resolved_path) = resolve_import_path(
            &module_path,
            &reference.file_path,
            reference.language,
            context,
        ) && resolved_path != reference.file_path
            && let Some(file_node) = context
                .get_nodes_in_file(&resolved_path)
                .into_iter()
                .find(|n| n.kind == NodeKind::File)
        {
            return Some(resolved(reference, &file_node.id, 0.9, ResolvedBy::Import));
        }
        if reference.language == Language::Python
            && let Some(mod_file) =
                find_python_module_file(&module_path, context, &reference.file_path)
        {
            return Some(resolved(reference, &mod_file.id, 0.9, ResolvedBy::Import));
        }
    }
    None
}

pub(super) fn resolve_path_import_to_file(
    reference: &UnresolvedRef,
    context: &mut dyn ResolutionContext,
) -> Option<ResolvedRef> {
    if reference.reference_kind != ReferenceKind::Imports
        || !(reference.reference_name.starts_with('.') || reference.reference_name.starts_with('/'))
    {
        return None;
    }
    let resolved_path = resolve_import_path(
        &reference.reference_name,
        &reference.file_path,
        reference.language,
        context,
    )?;
    if resolved_path == reference.file_path {
        return None;
    }
    let file_node = context
        .get_nodes_in_file(&resolved_path)
        .into_iter()
        .find(|node| node.kind == NodeKind::File)?;
    Some(resolved(reference, &file_node.id, 0.9, ResolvedBy::Import))
}

fn find_python_module_file(
    module_path: &str,
    context: &mut dyn ResolutionContext,
    exclude_file_path: &str,
) -> Option<Node> {
    // 不直接扫描文件系统，而是走已索引的 File 节点；这样解析结果和当前图快照
    // 保持一致，也能自然支持增量同步后的缓存。
    if module_path.is_empty() || module_path.starts_with('.') {
        return None;
    }
    let rel = module_path.replace('.', "/");
    let last_seg = module_path.rsplit('.').next()?;
    let module_file = context
        .get_nodes_by_name(&format!("{last_seg}.py"))
        .into_iter()
        .find(|node| {
            node.kind == NodeKind::File
                && node.file_path != exclude_file_path
                && ends_with_path(&node.file_path, &format!("{rel}.py"))
        });
    module_file.or_else(|| {
        context
            .get_nodes_by_name("__init__.py")
            .into_iter()
            .find(|node| {
                node.kind == NodeKind::File
                    && node.file_path != exclude_file_path
                    && ends_with_path(&node.file_path, &format!("{rel}/__init__.py"))
            })
    })
}

pub(super) fn resolve_python_absolute_module(
    reference: &UnresolvedRef,
    context: &mut dyn ResolutionContext,
) -> Option<ResolvedRef> {
    if reference.reference_kind != ReferenceKind::Imports || !reference.reference_name.contains('.')
    {
        return None;
    }
    let hit = find_python_module_file(&reference.reference_name, context, &reference.file_path)?;
    Some(resolved(reference, &hit.id, 0.9, ResolvedBy::Import))
}
