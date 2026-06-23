//! Import resolver shared helpers.
//!
//! 子模块会频繁在“源码里的路径”和“项目相对路径”之间转换；这里集中处理
//! 扩展名优先级、斜杠归一化和 ResolvedRef 构造，避免各语言各写一套。

use std::path::{Component, Path, PathBuf};

use crate::resolution::types::{ResolvedBy, ResolvedRef, UnresolvedRef};
use crate::types::Language;

pub(super) fn extension_resolution(language: Language) -> &'static [&'static str] {
    // 扩展名顺序就是解析优先级；例如 TS 先试 `.ts/.tsx` 再退到 JS，
    // 这样能更贴近 bundler/语言服务的默认行为。
    match language {
        Language::TypeScript => &[
            ".ts",
            ".tsx",
            ".d.ts",
            ".js",
            ".jsx",
            "/index.ts",
            "/index.tsx",
            "/index.js",
        ],
        Language::JavaScript => &[".js", ".jsx", ".mjs", ".cjs", "/index.js", "/index.jsx"],
        Language::Tsx => &[
            ".tsx",
            ".ts",
            ".d.ts",
            ".js",
            ".jsx",
            "/index.tsx",
            "/index.ts",
            "/index.js",
        ],
        Language::Jsx => &[".jsx", ".js", "/index.jsx", "/index.js"],
        Language::Svelte => &[
            ".ts",
            ".js",
            ".svelte",
            ".tsx",
            ".jsx",
            "/index.ts",
            "/index.js",
            "/index.svelte",
        ],
        Language::Vue => &[
            ".ts",
            ".js",
            ".vue",
            ".tsx",
            ".jsx",
            "/index.ts",
            "/index.js",
            "/index.vue",
        ],
        Language::Astro => &[
            ".ts",
            ".js",
            ".astro",
            ".tsx",
            ".jsx",
            "/index.ts",
            "/index.js",
            "/index.astro",
        ],
        Language::Python => &[".py", "/__init__.py"],
        Language::Go => &[".go"],
        Language::Rust => &[".rs", "/mod.rs"],
        Language::Java => &[".java"],
        Language::C => &[".h", ".c"],
        Language::Cpp => &[".h", ".hpp", ".hxx", ".cpp", ".cc", ".cxx"],
        Language::CSharp => &[".cs"],
        Language::Php => &[".php"],
        Language::Ruby => &[".rb"],
        Language::ObjC => &[".h", ".m", ".mm"],
        _ => &[],
    }
}

pub(super) fn shared_char_prefix(a: &str, b: &str) -> usize {
    a.chars().zip(b.chars()).take_while(|(x, y)| x == y).count()
}

pub(super) fn normalize_path_buf(path: PathBuf) -> PathBuf {
    // Path::canonicalize 需要文件真实存在；import 解析经常在候选路径上试探，
    // 所以这里做纯语法归一化。
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

pub(super) fn path_relative_to_project(
    project_root: impl AsRef<Path>,
    path: impl AsRef<Path>,
) -> Option<String> {
    let root = normalize_path_buf(project_root.as_ref().to_path_buf());
    let path = normalize_path_buf(path.as_ref().to_path_buf());
    let rel = path.strip_prefix(root).ok()?;
    Some(rel.to_string_lossy().replace('\\', "/"))
}

pub(super) fn normalize_slashes(path: String) -> String {
    let mut out = Vec::new();
    let normalized = path.replace('\\', "/");
    for part in normalized.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                out.pop();
            }
            _ => out.push(part),
        }
    }
    out.join("/")
}

pub(super) fn ends_with_path(path: &str, want: &str) -> bool {
    path == want || path.ends_with(&format!("/{want}"))
}

pub(super) fn resolved(
    reference: &UnresolvedRef,
    target_node_id: &str,
    confidence: f64,
    resolved_by: ResolvedBy,
) -> ResolvedRef {
    ResolvedRef {
        original: reference.clone(),
        target_node_id: target_node_id.to_string(),
        confidence,
        resolved_by,
    }
}
