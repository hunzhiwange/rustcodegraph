//! Go import binding resolution.
//!
//! Go 的选择器 `pkg.Symbol` 只有在 `pkg` 来自当前 module 内 import 时才解析到
//! 项目符号；标准库和三方包保持未解析，避免制造外部节点。

use crate::resolution::types::{
    ImportMapping, ResolutionContext, ResolvedBy, ResolvedRef, UnresolvedRef,
};
use crate::types::Language;

use super::common::resolved;

pub(super) fn resolve_go_cross_package_reference(
    reference: &UnresolvedRef,
    imports: &[ImportMapping],
    context: &mut dyn ResolutionContext,
) -> Option<ResolvedRef> {
    // Go 只导出首字母大写符号；这里依赖提取阶段的 `is_exported`，并用 package
    // 目录匹配 import source，避免同名导出跨包误连。
    let module = context.get_go_module()?;
    let dot_idx = reference.reference_name.find('.')?;
    if dot_idx == 0 {
        return None;
    }
    let receiver = &reference.reference_name[..dot_idx];
    let member_name = &reference.reference_name[dot_idx + 1..];
    if member_name.is_empty() {
        return None;
    }
    for imp in imports.iter().filter(|imp| imp.local_name == receiver) {
        if imp.source != module.module_path
            && !imp.source.starts_with(&format!("{}/", module.module_path))
        {
            continue;
        }
        let pkg_dir = if imp.source == module.module_path {
            String::new()
        } else {
            imp.source[module.module_path.len() + 1..].to_string()
        };
        for node in context.get_nodes_by_name(member_name) {
            if node.language != Language::Go || !node.is_exported.unwrap_or(false) {
                continue;
            }
            let fp = node.file_path.replace('\\', "/");
            let file_dir = fp.rsplit_once('/').map(|(dir, _)| dir).unwrap_or("");
            if file_dir == pkg_dir {
                return Some(resolved(reference, &node.id, 0.9, ResolvedBy::Import));
            }
        }
    }
    None
}
