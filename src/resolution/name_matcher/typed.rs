//! 基于接收者类型的 method resolution 辅助。
//!
//! method_call、chain 和 C++/JVM 特化策略都会调用这里：先按 `Type::method` 查找，
//! 必要时沿 supertypes 递归，并用 import FQN 消除 JVM 同名类型歧义。

use crate::resolution::types::{ResolutionContext, ResolvedBy, ResolvedRef, UnresolvedRef};
use crate::types::{Language, NodeKind};

use super::common::{resolved, strip_angle_generics};

#[allow(clippy::too_many_arguments)]
pub(super) fn resolve_method_on_type(
    type_name: &str,
    method_name: &str,
    reference: &UnresolvedRef,
    context: &mut dyn ResolutionContext,
    confidence: f64,
    resolved_by: ResolvedBy,
    preferred_fqn: Option<&str>,
    depth: usize,
) -> Option<ResolvedRef> {
    let want = format!("{type_name}::{method_name}");
    let matches = context
        .get_nodes_by_name(method_name)
        .into_iter()
        .filter(|node| {
            node.kind == NodeKind::Method
                && node.language == reference.language
                && (node.qualified_name == want
                    || node.qualified_name.ends_with(&format!("::{want}")))
        })
        .collect::<Vec<_>>();

    if matches.is_empty() {
        if depth < 4 {
            // 继承链递归由调用方提供的 supertypes 驱动，深度封顶避免环或框架基类网
            // 带来指数级查找。
            for supertype in context.get_supertypes(type_name, reference.language) {
                if let Some(via) = resolve_method_on_type(
                    &supertype,
                    method_name,
                    reference,
                    context,
                    confidence,
                    resolved_by,
                    preferred_fqn,
                    depth + 1,
                ) {
                    return Some(via);
                }
            }
        }
        return None;
    }

    if matches.len() > 1
        && let Some(preferred_fqn) = preferred_fqn
    {
        // JVM/Kotlin 同名类很常见；import 中的 FQN 能转换成文件路径后缀，用来挑选
        // 真正被当前文件导入的类型。
        let ext = if reference.language == Language::Kotlin {
            ".kt"
        } else {
            ".java"
        };
        let fqn_path = format!("{}{}", preferred_fqn.replace('.', "/"), ext);
        if let Some(chosen) = matches.iter().find(|node| {
            let fp = node.file_path.replace('\\', "/");
            fp.ends_with(&fqn_path) || fp.ends_with(&format!("/{fqn_path}"))
        }) {
            return Some(resolved(reference, &chosen.id, confidence, resolved_by));
        }
    }

    Some(resolved(reference, &matches[0].id, confidence, resolved_by))
}

pub(super) fn lookup_callee_return_type(
    callee: &str,
    reference: &UnresolvedRef,
    context: &mut dyn ResolutionContext,
) -> Option<String> {
    // 支持 `Class::factory` 和裸 `factory` 两种形状；只有 extractor 已记录返回类型
    // 的函数/方法才会参与后续链式调用解析。
    let mut method = callee.to_string();
    let mut cls = None;
    if callee.contains("::") {
        let parts = callee
            .split("::")
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>();
        method = parts.last()?.to_string();
        cls = Some(parts[..parts.len().saturating_sub(1)].join("::"));
    }

    let candidates = context
        .get_nodes_by_name(&method)
        .into_iter()
        .filter(|node| {
            (node.kind == NodeKind::Method || node.kind == NodeKind::Function)
                && node.language == reference.language
                && node.return_type.is_some()
        })
        .collect::<Vec<_>>();

    if let Some(cls) = cls {
        let want = format!("{cls}::{method}");
        return candidates
            .into_iter()
            .find(|node| {
                node.qualified_name == want
                    || node.qualified_name.ends_with(&format!("::{want}"))
                    || want.ends_with(&format!("::{}", node.qualified_name))
            })
            .and_then(|node| node.return_type);
    }

    candidates
        .into_iter()
        .find(|node| node.kind == NodeKind::Function)
        .and_then(|node| node.return_type)
}

pub(super) fn imported_fqn_of(
    type_name: &str,
    reference: &UnresolvedRef,
    context: &mut dyn ResolutionContext,
) -> Option<String> {
    context
        .get_import_mappings(&reference.file_path, reference.language)
        .into_iter()
        .find(|mapping| mapping.local_name == type_name)
        .map(|mapping| mapping.source)
}

pub(super) fn infer_java_field_receiver_type(
    receiver_name: &str,
    reference: &UnresolvedRef,
    context: &mut dyn ResolutionContext,
) -> Option<String> {
    // 只解析当前类/接口体内字段声明的签名，作为 Java/Kotlin `field.method()`
    // 的 receiver type 线索；局部变量和复杂泛型不在这个轻量路径里展开。
    let in_file = context.get_nodes_in_file(&reference.file_path);
    let enclosing = in_file
        .iter()
        .filter(|node| {
            (node.kind == NodeKind::Class || node.kind == NodeKind::Interface)
                && node.language == reference.language
                && node.start_line <= reference.line
                && node.end_line >= reference.line
        })
        .max_by_key(|node| node.start_line)?;
    let field = in_file.iter().find(|node| {
        node.kind == NodeKind::Field
            && node.name == receiver_name
            && node.language == reference.language
            && node.start_line >= enclosing.start_line
            && node.end_line <= enclosing.end_line
    })?;
    let signature = field.signature.as_ref()?;
    let before_name = signature[..signature.rfind(&field.name)?].trim();
    let without_generics = strip_angle_generics(before_name);
    let normalized = without_generics.replace("[]", "").replace("...", "");
    let last = normalized
        .split(['.', ' ', '\t'])
        .rfind(|part| !part.is_empty())?;
    last.chars()
        .next()
        .map(|ch| ch.is_ascii_uppercase())
        .unwrap_or(false)
        .then(|| last.to_string())
}
