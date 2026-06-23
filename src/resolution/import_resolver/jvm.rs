//! JVM import resolution for Java and Kotlin.
//!
//! 直接 import 通过 qualified_name 命中；普通引用再借助 import mapping 和文件路径
//! 兜底，兼容 Java/Kotlin 多平台 expect 类的邻近选择。

use crate::resolution::types::{
    ImportMapping, ResolutionContext, ResolvedBy, ResolvedRef, UnresolvedRef,
};
use crate::types::{Language, Node, ReferenceKind};

use super::common::resolved;

pub fn resolve_jvm_import(
    reference: &UnresolvedRef,
    context: &mut dyn ResolutionContext,
) -> Option<ResolvedRef> {
    if reference.reference_kind != ReferenceKind::Imports
        || !matches!(reference.language, Language::Java | Language::Kotlin)
    {
        return None;
    }
    let last_dot = reference.reference_name.rfind('.')?;
    if last_dot == 0 {
        return None;
    }
    let pkg = &reference.reference_name[..last_dot];
    let sym = &reference.reference_name[last_dot + 1..];
    if sym == "*" {
        return None;
    }
    let candidates = context.get_nodes_by_qualified_name(&format!("{pkg}::{sym}"));
    if candidates.is_empty() {
        return None;
    }
    let best = if candidates.len() == 1 {
        candidates[0].clone()
    } else {
        pick_closest_jvm_candidate(&candidates, &reference.file_path)
    };
    Some(resolved(reference, &best.id, 0.95, ResolvedBy::Import))
}

fn pick_closest_jvm_candidate(candidates: &[Node], from_path: &str) -> Node {
    // 多平台/生成代码可能产生相同 qualified_name；优先选择和引用文件目录前缀
    // 更接近的候选，同距离时偏向 Kotlin expect 声明。
    let from_dirs = from_path.split('/').collect::<Vec<_>>();
    let mut best = candidates[0].clone();
    let mut best_prox = shared_prefix_dirs(&best.file_path, &from_dirs);
    for cand in &candidates[1..] {
        let prox = shared_prefix_dirs(&cand.file_path, &from_dirs);
        let cand_expect = cand
            .decorators
            .as_ref()
            .map(|d| d.iter().any(|x| x == "expect"))
            .unwrap_or(false);
        let best_expect = best
            .decorators
            .as_ref()
            .map(|d| d.iter().any(|x| x == "expect"))
            .unwrap_or(false);
        if prox > best_prox || (prox == best_prox && cand_expect && !best_expect) {
            best = cand.clone();
            best_prox = prox;
        }
    }
    best
}

fn shared_prefix_dirs(path: &str, from_dirs: &[&str]) -> usize {
    let dirs = path.split('/').collect::<Vec<_>>();
    let max = dirs
        .len()
        .saturating_sub(1)
        .min(from_dirs.len().saturating_sub(1));
    (0..max)
        .take_while(|idx| dirs[*idx] == from_dirs[*idx])
        .count()
}

pub(super) fn resolve_java_imported_reference(
    reference: &UnresolvedRef,
    imports: &[ImportMapping],
    context: &mut dyn ResolutionContext,
) -> Option<ResolvedRef> {
    // 处理 `import a.b.Foo` 后的 `Foo` 或 `Foo.member`。对 bare import 还会回看
    // owner 文件，覆盖 inner/static member 被提取为独立节点的情况。
    if imports.is_empty() {
        return None;
    }
    let ext = if reference.language == Language::Kotlin {
        ".kt"
    } else {
        ".java"
    };
    for imp in imports {
        let matches_bare = imp.local_name == reference.reference_name;
        let matches_qualified = reference
            .reference_name
            .starts_with(&format!("{}.", imp.local_name));
        if !matches_bare && !matches_qualified {
            continue;
        }
        let fqn_path = format!("{}{}", imp.source.replace('.', "/"), ext);
        let member_name = if matches_bare {
            imp.local_name.clone()
        } else {
            reference.reference_name[imp.local_name.len() + 1..].to_string()
        };
        let candidates = context.get_nodes_by_name(&member_name);
        for node in &candidates {
            if node.language != reference.language {
                continue;
            }
            let fp = node.file_path.replace('\\', "/");
            if fp.ends_with(&fqn_path) || fp.ends_with(&format!("/{fqn_path}")) {
                return Some(resolved(reference, &node.id, 0.9, ResolvedBy::Import));
            }
        }
        if matches_bare && let Some(dot) = imp.source.rfind('.') {
            let owner_path = format!("{}{}", imp.source[..dot].replace('.', "/"), ext);
            for node in &candidates {
                if node.language != reference.language {
                    continue;
                }
                let fp = node.file_path.replace('\\', "/");
                if fp.ends_with(&owner_path) || fp.ends_with(&format!("/{owner_path}")) {
                    return Some(resolved(reference, &node.id, 0.9, ResolvedBy::Import));
                }
            }
        }
    }
    None
}
