//! C/C++ include path discovery and include resolution.
//!
//! 优先读取 compile_commands.json 获取真实 `-I`，没有编译数据库时才按常见目录
//! 启发式兜底；结果按项目根缓存，避免每个 include 都扫文件系统。

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

use crate::resolution::types::ResolutionContext;
use crate::types::Language;

use super::common::{extension_resolution, path_relative_to_project};

static CPP_INCLUDE_DIR_CACHE: LazyLock<Mutex<HashMap<String, Vec<String>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub fn clear_cpp_include_dir_cache() {
    CPP_INCLUDE_DIR_CACHE
        .lock()
        .expect("cpp cache poisoned")
        .clear();
}

pub fn load_cpp_include_dirs(project_root: impl AsRef<Path>) -> Vec<String> {
    let key = project_root.as_ref().to_string_lossy().into_owned();
    if let Some(cached) = CPP_INCLUDE_DIR_CACHE
        .lock()
        .expect("cpp cache poisoned")
        .get(&key)
        .cloned()
    {
        return cached;
    }

    let dirs = load_cpp_include_dirs_from_compile_db(project_root.as_ref())
        .unwrap_or_else(|| load_cpp_include_dirs_heuristic(project_root.as_ref()));
    CPP_INCLUDE_DIR_CACHE
        .lock()
        .expect("cpp cache poisoned")
        .insert(key, dirs.clone());
    dirs
}

fn load_cpp_include_dirs_from_compile_db(project_root: &Path) -> Option<Vec<String>> {
    // compile_commands 里的 directory 可以让相对 `-I` 精确落回项目目录；
    // 只保留项目内路径，系统头文件交给 built-in 过滤。
    let candidates = [
        project_root.join("compile_commands.json"),
        project_root.join("build/compile_commands.json"),
        project_root.join("cmake-build-debug/compile_commands.json"),
        project_root.join("cmake-build-release/compile_commands.json"),
        project_root.join("out/compile_commands.json"),
    ];
    let db_path = candidates.into_iter().find(|path| path.exists())?;
    let content = fs::read_to_string(db_path).ok()?;
    let entries = serde_json::from_str::<serde_json::Value>(&content)
        .ok()?
        .as_array()?
        .clone();
    let mut dirs = HashSet::new();
    for entry in entries {
        let directory = entry
            .get("directory")
            .and_then(|value| value.as_str())
            .unwrap_or_else(|| project_root.to_str().unwrap_or(""));
        let args =
            if let Some(arguments) = entry.get("arguments").and_then(|value| value.as_array()) {
                arguments
                    .iter()
                    .filter_map(|value| value.as_str().map(ToOwned::to_owned))
                    .collect::<Vec<_>>()
            } else {
                shlex_split(
                    entry
                        .get("command")
                        .and_then(|value| value.as_str())
                        .unwrap_or(""),
                )
            };
        let mut i = 0;
        while i < args.len() {
            let arg = &args[i];
            let include_dir =
                if let Some(rest) = arg.strip_prefix("-I").filter(|rest| !rest.is_empty()) {
                    Some(rest.to_string())
                } else if (arg == "-isystem" || arg == "-I") && i + 1 < args.len() {
                    i += 1;
                    Some(args[i].clone())
                } else {
                    None
                };
            if let Some(include_dir) = include_dir {
                let abs = if Path::new(&include_dir).is_absolute() {
                    PathBuf::from(include_dir)
                } else {
                    Path::new(directory).join(include_dir)
                };
                if let Some(rel) = path_relative_to_project(project_root, &abs)
                    && !rel.is_empty()
                    && !rel.starts_with("..")
                {
                    dirs.insert(rel);
                }
            }
            i += 1;
        }
    }
    Some(dirs.into_iter().collect())
}

fn shlex_split(cmd: &str) -> Vec<String> {
    // compile_commands 可能只有 command 字符串；这个简化版 shell split 只覆盖
    // 引号和反斜杠，足够解析常见 compiler flags。
    let mut result = Vec::new();
    let mut chars = cmd.chars().peekable();
    while chars.peek().is_some() {
        while chars.peek().map(|ch| ch.is_whitespace()).unwrap_or(false) {
            chars.next();
        }
        let Some(ch) = chars.peek().copied() else {
            break;
        };
        if ch == '"' || ch == '\'' {
            let quote = chars.next().unwrap();
            let mut arg = String::new();
            while let Some(next) = chars.next() {
                if next == quote {
                    break;
                }
                if next == '\\' && quote == '"' {
                    if let Some(escaped) = chars.next() {
                        arg.push(escaped);
                    }
                } else {
                    arg.push(next);
                }
            }
            result.push(arg);
        } else {
            let mut arg = String::new();
            while chars.peek().map(|ch| !ch.is_whitespace()).unwrap_or(false) {
                arg.push(chars.next().unwrap());
            }
            result.push(arg);
        }
    }
    result
}

fn load_cpp_include_dirs_heuristic(project_root: &Path) -> Vec<String> {
    // 没有编译数据库时，扫描少量顶层目录而不是递归全仓库，避免大 C++ 项目
    // 在初始化 include 路径时变慢。
    let mut dirs = Vec::new();
    let convention_dirs = ["include", "src", "lib", "api", "inc"];
    let Ok(entries) = fs::read_dir(project_root) else {
        return dirs;
    };
    for entry in entries.filter_map(Result::ok) {
        if !entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false) {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if convention_dirs.contains(&name.to_ascii_lowercase().as_str()) {
            dirs.push(name);
            continue;
        }
        if fs::read_dir(entry.path())
            .ok()
            .into_iter()
            .flat_map(|entries| entries.filter_map(Result::ok))
            .any(|file| {
                file.file_name()
                    .to_string_lossy()
                    .to_ascii_lowercase()
                    .ends_with(".h")
                    || file
                        .file_name()
                        .to_string_lossy()
                        .to_ascii_lowercase()
                        .ends_with(".hpp")
                    || file
                        .file_name()
                        .to_string_lossy()
                        .to_ascii_lowercase()
                        .ends_with(".hxx")
                    || file
                        .file_name()
                        .to_string_lossy()
                        .to_ascii_lowercase()
                        .ends_with(".hh")
            })
        {
            dirs.push(name);
        }
    }
    dirs
}

pub(super) fn resolve_cpp_include_path(
    import_path: &str,
    language: Language,
    context: &mut dyn ResolutionContext,
) -> Option<String> {
    let include_dirs = context.get_cpp_include_dirs();
    let extensions = extension_resolution(language);
    for dir in include_dirs {
        let normalized = dir.replace('\\', "/");
        for ext in extensions {
            let candidate = format!("{normalized}/{import_path}{ext}");
            if context.file_exists(&candidate) {
                return Some(candidate);
            }
        }
        let candidate = format!("{normalized}/{import_path}");
        if context.file_exists(&candidate) {
            return Some(candidate);
        }
    }
    None
}
