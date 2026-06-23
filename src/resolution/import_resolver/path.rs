//! Import path resolution.
//!
//! 只把项目内路径解析为文件；外部包、标准库和框架虚拟模块由更上层的 built-in
//! 或 framework resolver 消费，避免把不存在的文件写成边。

use std::path::{Path, PathBuf};

use crate::resolution::path_aliases::apply_aliases;
use crate::resolution::types::ResolutionContext;
use crate::resolution::workspace_packages::resolve_workspace_import;
use crate::types::Language;

use super::common::{extension_resolution, normalize_path_buf, path_relative_to_project};
use super::cpp::resolve_cpp_include_path;

pub fn resolve_import_path(
    import_path: &str,
    from_file: &str,
    language: Language,
    context: &mut dyn ResolutionContext,
) -> Option<String> {
    // 解析顺序很重要：相对路径最确定，其次 tsconfig/workspace alias，
    // 最后才让 C/C++ include path 兜底。
    if is_external_import(import_path, language, Some(context)) {
        return None;
    }

    let project_root = context.get_project_root();
    let from_dir = Path::new(&project_root)
        .join(from_file)
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from(&project_root));

    if import_path.starts_with('.') {
        return resolve_relative_import(import_path, &from_dir, language, context);
    }

    if let Some(aliased) = resolve_aliased_import(import_path, &project_root, language, context) {
        return Some(aliased);
    }

    if matches!(language, Language::C | Language::Cpp) {
        return resolve_cpp_include_path(import_path, language, context);
    }

    None
}

const C_CPP_STDLIB_HEADERS: &[&str] = &[
    "assert.h",
    "complex.h",
    "ctype.h",
    "errno.h",
    "fenv.h",
    "float.h",
    "inttypes.h",
    "iso646.h",
    "limits.h",
    "locale.h",
    "math.h",
    "setjmp.h",
    "signal.h",
    "stdalign.h",
    "stdarg.h",
    "stdatomic.h",
    "stdbool.h",
    "stddef.h",
    "stdint.h",
    "stdio.h",
    "stdlib.h",
    "stdnoreturn.h",
    "string.h",
    "tgmath.h",
    "threads.h",
    "time.h",
    "uchar.h",
    "wchar.h",
    "wctype.h",
    "cassert",
    "ccomplex",
    "cctype",
    "cerrno",
    "cfenv",
    "cfloat",
    "cinttypes",
    "ciso646",
    "climits",
    "clocale",
    "cmath",
    "csetjmp",
    "csignal",
    "cstdalign",
    "cstdarg",
    "cstdbool",
    "cstddef",
    "cstdint",
    "cstdio",
    "cstdlib",
    "cstring",
    "ctgmath",
    "ctime",
    "cuchar",
    "cwchar",
    "cwctype",
    "algorithm",
    "any",
    "array",
    "atomic",
    "barrier",
    "bit",
    "bitset",
    "charconv",
    "chrono",
    "codecvt",
    "compare",
    "complex",
    "concepts",
    "condition_variable",
    "coroutine",
    "deque",
    "exception",
    "execution",
    "expected",
    "filesystem",
    "format",
    "forward_list",
    "fstream",
    "functional",
    "future",
    "generator",
    "initializer_list",
    "iomanip",
    "ios",
    "iosfwd",
    "iostream",
    "istream",
    "iterator",
    "latch",
    "limits",
    "list",
    "locale",
    "map",
    "mdspan",
    "memory",
    "memory_resource",
    "mutex",
    "new",
    "numbers",
    "numeric",
    "optional",
    "ostream",
    "print",
    "queue",
    "random",
    "ranges",
    "ratio",
    "regex",
    "scoped_allocator",
    "semaphore",
    "set",
    "shared_mutex",
    "source_location",
    "span",
    "spanstream",
    "sstream",
    "stack",
    "stacktrace",
    "stdexcept",
    "stdfloat",
    "stop_token",
    "streambuf",
    "string",
    "string_view",
    "strstream",
    "syncstream",
    "system_error",
    "thread",
    "tuple",
    "type_traits",
    "typeindex",
    "typeinfo",
    "unordered_map",
    "unordered_set",
    "utility",
    "valarray",
    "variant",
    "vector",
    "version",
];

fn is_external_import(
    import_path: &str,
    language: Language,
    mut context: Option<&mut dyn ResolutionContext>,
) -> bool {
    // 这个判断是“是否不该走文件路径解析”，不是完整包管理器实现。判外部时
    // 要先给 workspace/alias 放行，否则 monorepo 内部包会被误跳过。
    if import_path.starts_with('.') {
        return false;
    }

    if let Some(ctx) = context.as_deref_mut()
        && let Some(workspaces) = ctx.get_workspace_packages()
        && resolve_workspace_import(import_path, &workspaces).is_some()
    {
        return false;
    }

    if matches!(
        language,
        Language::TypeScript | Language::JavaScript | Language::Tsx | Language::Jsx
    ) {
        const NODE_BUILT_INS: &[&str] = &[
            "fs",
            "path",
            "os",
            "crypto",
            "http",
            "https",
            "url",
            "util",
            "events",
            "stream",
            "child_process",
            "buffer",
        ];
        if NODE_BUILT_INS.contains(&import_path) {
            return true;
        }
        if let Some(ctx) = context.as_deref_mut()
            && let Some(aliases) = ctx.get_project_aliases()
            && aliases
                .patterns
                .iter()
                .any(|pattern| import_path.starts_with(&pattern.prefix))
        {
            return false;
        }
        if !import_path.starts_with("@/")
            && !import_path.starts_with("~/")
            && !import_path.starts_with("src/")
        {
            return true;
        }
    }

    if language == Language::Python {
        const STDLIBS: &[&str] = &[
            "os",
            "sys",
            "json",
            "re",
            "math",
            "datetime",
            "collections",
            "typing",
            "pathlib",
            "logging",
        ];
        if STDLIBS.contains(&import_path.split('.').next().unwrap_or(import_path)) {
            return true;
        }
    }

    if language == Language::Go {
        if import_path.starts_with('.') {
            return false;
        }
        if let Some(ctx) = context
            && let Some(module) = ctx.get_go_module()
            && (import_path == module.module_path
                || import_path.starts_with(&format!("{}/", module.module_path)))
        {
            return false;
        }
        if import_path.contains("/internal/") {
            return false;
        }
        return true;
    }

    if matches!(language, Language::C | Language::Cpp) {
        if C_CPP_STDLIB_HEADERS.contains(&import_path) {
            return true;
        }
        if let Some(without_ext) = import_path.strip_suffix(".h")
            && C_CPP_STDLIB_HEADERS.contains(&without_ext)
        {
            return true;
        }
    }

    false
}

fn resolve_relative_import(
    import_path: &str,
    from_dir: &Path,
    language: Language,
    context: &mut dyn ResolutionContext,
) -> Option<String> {
    // Python 相对 import 的点数表示包层级，不等同于文件系统 `./`；
    // 单独转换后再走通用扩展名探测。
    let project_root = context.get_project_root();
    let extensions = extension_resolution(language);

    if language == Language::Python && import_path.starts_with('.') {
        let dots = import_path.chars().take_while(|ch| *ch == '.').count();
        let mut rel = String::new();
        for _ in 0..dots.saturating_sub(1) {
            rel.push_str("../");
        }
        rel.push_str(&import_path[dots..].replace('.', "/"));
        let py_base = normalize_path_buf(from_dir.join(rel));
        let py_rel = path_relative_to_project(&project_root, &py_base)?;
        for ext in extensions {
            let candidate = format!("{py_rel}{ext}");
            if context.file_exists(&candidate) {
                return Some(candidate);
            }
        }
        if !py_rel.is_empty() && context.file_exists(&py_rel) {
            return Some(py_rel);
        }
        return None;
    }

    let base = normalize_path_buf(from_dir.join(import_path));
    let relative_path = path_relative_to_project(&project_root, &base)?;
    for ext in extensions {
        let candidate = format!("{relative_path}{ext}");
        if context.file_exists(&candidate) {
            return Some(candidate);
        }
    }
    context.file_exists(&relative_path).then_some(relative_path)
}

fn resolve_aliased_import(
    import_path: &str,
    project_root: &str,
    language: Language,
    context: &mut dyn ResolutionContext,
) -> Option<String> {
    // 先使用项目配置 alias/workspace package，再尝试常见社区别名；最后允许
    // 已经是项目相对路径的 import 直接命中。
    let extensions = extension_resolution(language);
    fn try_with_ext(
        base_path: &str,
        extensions: &[&str],
        context: &mut dyn ResolutionContext,
    ) -> Option<String> {
        for ext in extensions {
            let candidate = format!("{base_path}{ext}");
            if context.file_exists(&candidate) {
                return Some(candidate);
            }
        }
        context
            .file_exists(base_path)
            .then(|| base_path.to_string())
    }

    if let Some(alias_map) = context.get_project_aliases() {
        for candidate in apply_aliases(import_path, &alias_map, project_root) {
            if let Some(hit) = try_with_ext(&candidate, extensions, context) {
                return Some(hit);
            }
        }
    }

    if let Some(workspaces) = context.get_workspace_packages()
        && let Some(base) = resolve_workspace_import(import_path, &workspaces)
        && let Some(hit) = try_with_ext(&base, extensions, context)
    {
        return Some(hit);
    }

    for (alias, replacement) in [
        ("@/", "src/"),
        ("~/", "src/"),
        ("@src/", "src/"),
        ("src/", "src/"),
        ("@app/", "app/"),
        ("app/", "app/"),
    ] {
        if import_path.starts_with(alias)
            && let Some(hit) = try_with_ext(
                &import_path.replacen(alias, replacement, 1),
                extensions,
                context,
            )
        {
            return Some(hit);
        }
    }

    try_with_ext(import_path, extensions, context)
}
