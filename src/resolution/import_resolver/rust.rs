//! Rust module path and `use` reference resolution.
//!
//! Rust 的 `crate/self/super` 与文件布局强绑定。这里按源码文件位置推导模块文件，
//! 再在目标文件内找具体符号。

use std::path::{Path, PathBuf};

use crate::resolution::types::{
    ImportMapping, ResolutionContext, ResolvedBy, ResolvedRef, UnresolvedRef,
};
use crate::types::NodeKind;

use super::common::{path_relative_to_project, resolved};

pub(super) fn resolve_rust_path_reference(
    reference: &UnresolvedRef,
    context: &mut dyn ResolutionContext,
) -> Option<ResolvedRef> {
    let segments = reference
        .reference_name
        .split("::")
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();
    if segments.len() < 2 {
        return None;
    }
    let leaf = *segments.last()?;
    let file = resolve_rust_module_file(
        &segments[..segments.len() - 1],
        &reference.file_path,
        context,
    )?;
    if file == reference.file_path {
        return None;
    }
    let target = context.get_nodes_in_file(&file).into_iter().find(|node| {
        node.name == leaf
            && matches!(
                node.kind,
                NodeKind::Function
                    | NodeKind::Struct
                    | NodeKind::Enum
                    | NodeKind::Trait
                    | NodeKind::TypeAlias
                    | NodeKind::Constant
                    | NodeKind::Method
                    | NodeKind::Class
                    | NodeKind::Interface
            )
    })?;
    Some(resolved(reference, &target.id, 0.9, ResolvedBy::Import))
}

pub(super) fn resolve_rust_imported_reference(
    reference: &UnresolvedRef,
    imports: &[ImportMapping],
    context: &mut dyn ResolutionContext,
) -> Option<ResolvedRef> {
    // `use foo::Bar as Baz`、namespace use 和 `Baz::method` 都先还原到 module path，
    // 再在对应文件中找叶子符号。
    for imp in imports {
        let matches_bare = imp.local_name == reference.reference_name;
        let scoped_prefix = format!("{}::", imp.local_name);
        let scoped_tail = reference.reference_name.strip_prefix(&scoped_prefix);
        if !matches_bare && scoped_tail.is_none() {
            continue;
        }

        let (module_path, target_name) = if let Some(tail) = scoped_tail {
            let module_path = if imp.is_namespace {
                imp.source.clone()
            } else if imp.source.is_empty() {
                imp.exported_name.clone()
            } else {
                format!("{}::{}", imp.source, imp.exported_name)
            };
            let target_name = tail.rsplit("::").next().unwrap_or(tail).to_string();
            (module_path, target_name)
        } else if imp.is_namespace || imp.exported_name == "*" {
            continue;
        } else {
            (imp.source.clone(), imp.exported_name.clone())
        };

        let segments = module_path
            .split("::")
            .filter(|segment| !segment.is_empty())
            .collect::<Vec<_>>();
        if segments.is_empty() {
            continue;
        }
        let Some(file) = resolve_rust_module_file(&segments, &reference.file_path, context) else {
            continue;
        };
        if let Some(target) = context.get_nodes_in_file(&file).into_iter().find(|node| {
            node.name == target_name
                && matches!(
                    node.kind,
                    NodeKind::Function
                        | NodeKind::Struct
                        | NodeKind::Enum
                        | NodeKind::Trait
                        | NodeKind::TypeAlias
                        | NodeKind::Constant
                        | NodeKind::Method
                        | NodeKind::Class
                        | NodeKind::Interface
                )
        }) {
            return Some(resolved(reference, &target.id, 0.9, ResolvedBy::Import));
        }
    }
    None
}

fn rust_crate_root_dir(
    from_file_abs: &Path,
    context: &mut dyn ResolutionContext,
) -> Option<PathBuf> {
    // 从当前文件向上找 lib.rs/main.rs，比直接假设 project_root/src 更适合
    // workspace 和嵌套 crate。
    let project_root = context.get_project_root();
    let mut dir = from_file_abs.parent()?.to_path_buf();
    for _ in 0..64 {
        if context.file_exists(&path_relative_to_project(
            &project_root,
            dir.join("lib.rs"),
        )?) || context.file_exists(&path_relative_to_project(
            &project_root,
            dir.join("main.rs"),
        )?) {
            return Some(dir);
        }
        let parent = dir.parent()?.to_path_buf();
        if parent == dir {
            return None;
        }
        dir = parent;
    }
    None
}

fn rust_self_module_dir(from_file_abs: &Path) -> PathBuf {
    // `self::foo` 在 `mod.rs/lib.rs/main.rs` 中从所在目录开始；在普通 `bar.rs`
    // 中则从隐式 `bar/` 子模块目录开始。
    let base = from_file_abs
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    let dir = from_file_abs.parent().unwrap_or_else(|| Path::new(""));
    if matches!(base, "mod.rs" | "lib.rs" | "main.rs") {
        dir.to_path_buf()
    } else {
        dir.join(base.trim_end_matches(".rs"))
    }
}

fn resolve_rust_module_file(
    segments: &[&str],
    from_file: &str,
    context: &mut dyn ResolutionContext,
) -> Option<String> {
    // 同时支持 `foo.rs` 和 `foo/mod.rs`，并按 `crate/self/super` 语义选择起点。
    if segments.is_empty() {
        return None;
    }
    let project_root = context.get_project_root();
    let from_abs = Path::new(&project_root).join(from_file);

    fn resolve_under(
        start_dir: Option<PathBuf>,
        rest: &[&str],
        project_root: &str,
        context: &mut dyn ResolutionContext,
    ) -> Option<String> {
        let mut dir = start_dir?;
        let mut target_file = None;
        for seg in rest {
            if matches!(*seg, "self" | "crate" | "super") {
                continue;
            }
            let as_file = path_relative_to_project(project_root, dir.join(format!("{seg}.rs")))?;
            let as_mod = path_relative_to_project(project_root, dir.join(seg).join("mod.rs"))?;
            if context.file_exists(&as_file) {
                target_file = Some(as_file);
            } else if context.file_exists(&as_mod) {
                target_file = Some(as_mod);
            } else {
                return None;
            }
            dir = dir.join(seg);
        }
        target_file
    }

    match segments[0] {
        "crate" => resolve_under(
            rust_crate_root_dir(&from_abs, context),
            &segments[1..],
            &project_root,
            context,
        ),
        "self" => resolve_under(
            Some(rust_self_module_dir(&from_abs)),
            &segments[1..],
            &project_root,
            context,
        ),
        "super" => {
            let supers = segments.iter().take_while(|seg| **seg == "super").count();
            let mut dir = rust_self_module_dir(&from_abs);
            for _ in 0..supers {
                dir = dir.parent()?.to_path_buf();
            }
            resolve_under(Some(dir), &segments[supers..], &project_root, context)
        }
        _ => resolve_under(
            Some(rust_self_module_dir(&from_abs)),
            segments,
            &project_root,
            context,
        )
        .or_else(|| {
            resolve_under(
                rust_crate_root_dir(&from_abs, context),
                segments,
                &project_root,
                context,
            )
        }),
    }
}
