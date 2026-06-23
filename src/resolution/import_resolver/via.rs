//! Import-based reference resolution coordinator.
//!
//! 这是 import resolver 的主入口：先处理确定性最高的语言特例，再落到统一的
//! ImportMapping + export 查找流程。

use std::collections::HashSet;

use crate::resolution::types::{ResolutionContext, ResolvedBy, ResolvedRef, UnresolvedRef};
use crate::types::{Language, NodeKind, ReferenceKind};

use super::common::{normalize_slashes, resolved};
use super::exports::{SymbolWant, find_exported_symbol, resolve_static_member};
use super::go::resolve_go_cross_package_reference;
use super::jvm::resolve_java_imported_reference;
use super::path::resolve_import_path;
use super::php::{is_php_include_path_ref, resolve_php_include_path};
use super::python::{
    resolve_module_import_to_file, resolve_path_import_to_file, resolve_python_absolute_module,
    resolve_python_module_member,
};
use super::rust::{resolve_rust_imported_reference, resolve_rust_path_reference};
use super::script_requires::{resolve_lua_require, resolve_ruby_require};

pub fn resolve_via_import(
    reference: &UnresolvedRef,
    context: &mut dyn ResolutionContext,
) -> Option<ResolvedRef> {
    // C/C++ include 通常引用文件而不是符号。先试同目录精确文件，再使用 include
    // path 搜索，能减少大型项目里重名 header 的误连。
    if matches!(reference.language, Language::C | Language::Cpp)
        && reference.reference_kind == ReferenceKind::Imports
    {
        let from_dir = reference
            .file_path
            .rsplit_once('/')
            .map(|(dir, _)| dir)
            .unwrap_or("");
        let sibling_path = normalize_slashes(if from_dir.is_empty() {
            reference.reference_name.clone()
        } else {
            format!("{from_dir}/{}", reference.reference_name)
        });
        let sibling_base = sibling_path.rsplit('/').next().unwrap_or(&sibling_path);
        if let Some(sibling) = context
            .get_nodes_by_name(sibling_base)
            .into_iter()
            .find(|node| node.kind == NodeKind::File && node.file_path == sibling_path)
        {
            return Some(resolved(reference, &sibling.id, 0.92, ResolvedBy::Import));
        }
        let resolved_path = resolve_import_path(
            &reference.reference_name,
            &reference.file_path,
            reference.language,
            context,
        )?;
        let basename = resolved_path.rsplit('/').next()?;
        let file_node = context
            .get_nodes_by_name(basename)
            .into_iter()
            .find(|node| node.kind == NodeKind::File && node.file_path == resolved_path)?;
        return Some(resolved(reference, &file_node.id, 0.9, ResolvedBy::Import));
    }

    if is_php_include_path_ref(reference) {
        let resolved_path =
            resolve_php_include_path(&reference.reference_name, &reference.file_path, context)?;
        let basename = resolved_path.rsplit('/').next()?;
        let file_node = context
            .get_nodes_by_name(basename)
            .into_iter()
            .find(|node| node.kind == NodeKind::File && node.file_path == resolved_path)?;
        return Some(resolved(reference, &file_node.id, 0.9, ResolvedBy::Import));
    }

    let imports = context.get_import_mappings(&reference.file_path, reference.language);
    if imports.is_empty() && context.read_file(&reference.file_path).is_none() {
        return None;
    }

    // 语言专用解析放在通用 export 查找前：这些分支利用 package/module 语义，
    // 比单纯 local_name 匹配更可靠。
    if reference.language == Language::Go
        && let Some(go_result) = resolve_go_cross_package_reference(reference, &imports, context)
    {
        return Some(go_result);
    }

    if matches!(reference.language, Language::Java | Language::Kotlin)
        && let Some(java_result) = resolve_java_imported_reference(reference, &imports, context)
    {
        return Some(java_result);
    }

    if reference.language == Language::Rust
        && let Some(rust_import) = resolve_rust_imported_reference(reference, &imports, context)
    {
        return Some(rust_import);
    }

    if reference.language == Language::Python {
        if let Some(py_result) = resolve_python_module_member(reference, &imports, context) {
            return Some(py_result);
        }
        if let Some(py_mod) = resolve_python_absolute_module(reference, context) {
            return Some(py_mod);
        }
    }

    if reference.language == Language::Rust
        && reference.reference_name.contains("::")
        && let Some(rust_result) = resolve_rust_path_reference(reference, context)
    {
        return Some(rust_result);
    }

    if matches!(reference.language, Language::Lua | Language::Luau)
        && reference.reference_kind == ReferenceKind::Imports
        && let Some(lua_result) = resolve_lua_require(reference, context)
    {
        return Some(lua_result);
    }

    if reference.language == Language::Ruby
        && reference.reference_kind == ReferenceKind::Imports
        && let Some(ruby_result) = resolve_ruby_require(reference, context)
    {
        return Some(ruby_result);
    }

    if matches!(
        reference.language,
        Language::Python
            | Language::TypeScript
            | Language::Tsx
            | Language::JavaScript
            | Language::Jsx
    ) {
        if let Some(module_path_file) = resolve_path_import_to_file(reference, context) {
            return Some(module_path_file);
        }
        if let Some(module_file) = resolve_module_import_to_file(reference, &imports, context) {
            return Some(module_file);
        }
    }

    for imp in imports {
        // 通用 JS/TS 等 import 绑定：本地名先解析到源文件，再按 default/named/
        // namespace 语义在导出链上找最终节点。
        if imp.local_name == reference.reference_name
            || reference
                .reference_name
                .starts_with(&format!("{}.", imp.local_name))
        {
            let resolved_path = resolve_import_path(
                &imp.source,
                &reference.file_path,
                reference.language,
                context,
            )?;
            let exported_name = if imp.is_default {
                "default".to_string()
            } else {
                imp.exported_name.clone()
            };
            let member_name = imp.is_namespace.then(|| {
                reference
                    .reference_name
                    .trim_start_matches(&format!("{}.", imp.local_name))
                    .to_string()
            });
            let target = find_exported_symbol(
                &resolved_path,
                SymbolWant {
                    is_default: imp.is_default,
                    is_namespace: imp.is_namespace,
                    exported_name,
                    member_name,
                },
                reference.language,
                context,
                &mut HashSet::new(),
                0,
            )?;
            if !imp.is_namespace
                && reference
                    .reference_name
                    .starts_with(&format!("{}.", imp.local_name))
                && let Some(member) =
                    resolve_static_member(&target, reference, &imp.local_name, context)
            {
                return Some(resolved(reference, &member.id, 0.9, ResolvedBy::Import));
            }
            return Some(resolved(reference, &target.id, 0.9, ResolvedBy::Import));
        }
    }

    None
}
