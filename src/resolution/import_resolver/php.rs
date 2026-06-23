//! PHP include/require path resolution.
//!
//! PHP 的 `include`/`require` 可以直接引用路径字符串；这条路径解析和 class
//! import 不同，只尝试当前文件相对位置与 `.php` 扩展名。

use std::path::Path;

use crate::resolution::types::{ResolutionContext, UnresolvedRef};
use crate::types::{Language, ReferenceKind};

use super::common::{extension_resolution, path_relative_to_project};

pub fn is_php_include_path_ref(reference: &UnresolvedRef) -> bool {
    reference.language == Language::Php
        && reference.reference_kind == ReferenceKind::Imports
        && (reference.reference_name.contains('/') || reference.reference_name.contains('.'))
}

pub(super) fn resolve_php_include_path(
    include_path: &str,
    from_file: &str,
    context: &mut dyn ResolutionContext,
) -> Option<String> {
    // 先尊重源码里的精确路径，再补 `.php`，避免 `config.php` 这类显式文件名
    // 被重复拼接。
    let project_root = context.get_project_root();
    let from_dir = Path::new(&project_root)
        .join(from_file)
        .parent()
        .map(Path::to_path_buf)?;
    let relative = path_relative_to_project(&project_root, from_dir.join(include_path))?;
    if context.file_exists(&relative) {
        return Some(relative);
    }
    for ext in extension_resolution(Language::Php) {
        let candidate = format!("{relative}{ext}");
        if context.file_exists(&candidate) {
            return Some(candidate);
        }
    }
    None
}
