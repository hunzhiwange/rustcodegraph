//! Directory management translated from `directory.ts`.
//!
//! 这里统一管理每个项目下的 RustCodeGraph 数据目录，包括初始化、删除、
//! 体积统计和安全校验。调用方传入项目根目录，本模块负责拼出实际数据路径。

use std::env;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::sync::LazyLock;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::errors::{CodeGraphError, ConfigError, FileError};

const DEFAULT_RUSTCODEGRAPH_DIR: &str = ".rustcodegraph";
static WARNED_BAD_DIR_NAME: AtomicBool = AtomicBool::new(false);

pub fn code_graph_dir_name() -> String {
    // 只允许通过环境变量改“目录名”，不允许传完整路径；这样所有数据仍被
    // 锁在 project_root 下面，避免 init/uninit/watch 误操作项目外目录。
    let (env_name, raw) = env::var("RUSTCODEGRAPH_DIR")
        .map(|value| ("RUSTCODEGRAPH_DIR", value))
        .map(|(name, value)| (name, value.trim().to_string()))
        .unwrap_or(("RUSTCODEGRAPH_DIR", String::new()));
    if raw.is_empty() {
        return DEFAULT_RUSTCODEGRAPH_DIR.to_string();
    }

    let invalid = raw == "."
        || raw.contains("..")
        || raw.contains('/')
        || raw.contains('\\')
        || Path::new(&raw).is_absolute();

    if invalid {
        if !WARNED_BAD_DIR_NAME.swap(true, Ordering::SeqCst) {
            eprintln!(
                "[rustcodegraph] Ignoring invalid {env_name}=\"{raw}\" - it must be a plain \
                 directory name (no path separators, no \"..\", not absolute). Using \
                 \"{DEFAULT_RUSTCODEGRAPH_DIR}\"."
            );
        }
        return DEFAULT_RUSTCODEGRAPH_DIR.to_string();
    }

    raw
}

pub static RUSTCODEGRAPH_DIR: LazyLock<String> = LazyLock::new(code_graph_dir_name);

pub fn is_code_graph_data_dir(name: &str) -> bool {
    name == DEFAULT_RUSTCODEGRAPH_DIR
        || name == code_graph_dir_name()
        || name.starts_with(&(DEFAULT_RUSTCODEGRAPH_DIR.to_string() + "-"))
}

pub fn get_code_graph_dir(project_root: impl AsRef<Path>) -> PathBuf {
    project_root.as_ref().join(code_graph_dir_name())
}

pub fn is_initialized(project_root: impl AsRef<Path>) -> bool {
    let rustcodegraph_dir = get_code_graph_dir(project_root);
    if !rustcodegraph_dir.exists() {
        return false;
    }
    match fs::metadata(&rustcodegraph_dir) {
        Ok(metadata) if metadata.is_dir() => rustcodegraph_dir.join("rustcodegraph.db").exists(),
        _ => false,
    }
}

pub fn unsafe_index_root_reason(project_root: impl AsRef<Path>) -> Option<String> {
    let resolved = resolve_existing_or_lexical(project_root.as_ref());

    // 索引文件系统根或 home 父目录会产生巨大扫描面，也更容易把敏感文件
    // 写入本地索引；这些拒绝原因会直接展示给 CLI/MCP 用户。
    if is_filesystem_root(&resolved) {
        return Some("the filesystem root".to_string());
    }

    let home = resolve_existing_or_lexical(&home_dir()?);
    let resolved_norm = normalize_for_platform(&resolved);
    let home_norm = normalize_for_platform(&home);

    if resolved_norm == home_norm {
        return Some("your home directory".to_string());
    }

    let resolved_with_sep = append_separator(&resolved_norm);
    if home_norm.starts_with(&resolved_with_sep) {
        return Some("a parent of your home directory".to_string());
    }

    None
}

pub fn find_nearest_code_graph_root(start_path: impl AsRef<Path>) -> Option<PathBuf> {
    let mut current = absolutize(start_path.as_ref());

    loop {
        if is_initialized(&current) {
            return Some(current);
        }

        let Some(parent) = current.parent() else {
            break;
        };
        if parent == current {
            break;
        }
        current = parent.to_path_buf();
    }

    None
}

const GITIGNORE_CONTENT: &str = "# RustCodeGraph data files - local to each machine, not for committing.\n\
# Ignore everything in .rustcodegraph/ except this file itself, so transient\n\
# files (the database, daemon.pid, sockets, logs) never show up in git.\n\
*\n\
!.gitignore\n";

const GITIGNORE_MARKER: &str = "# RustCodeGraph data files";

fn is_stale_default_gitignore(content: &str) -> bool {
    if !content.trim_start().starts_with(GITIGNORE_MARKER) {
        return false;
    }
    !content.lines().any(|line| line.trim() == "*")
}

fn ensure_gitignore(gitignore_path: &Path) -> bool {
    // 早期版本写过只忽略 db 的 gitignore；检测到托管块但缺少 `*` 时升级。
    // 非托管的用户自定义内容则原样保留。
    let existing = fs::read_to_string(gitignore_path).ok();
    if existing
        .as_deref()
        .is_some_and(|content| !is_stale_default_gitignore(content))
    {
        return true;
    }
    fs::write(gitignore_path, GITIGNORE_CONTENT).is_ok()
}

pub fn create_directory(project_root: impl AsRef<Path>) -> Result<(), CodeGraphError> {
    let project_root = project_root.as_ref();
    let rustcodegraph_dir = get_code_graph_dir(project_root);
    let db_path = rustcodegraph_dir.join("rustcodegraph.db");

    if db_path.exists() {
        return Err(ConfigError::new(
            format!(
                "RustCodeGraph already initialized in {}",
                project_root.display()
            ),
            None,
        )
        .into());
    }

    fs::create_dir_all(&rustcodegraph_dir).map_err(|err| {
        file_error(
            "Failed to create RustCodeGraph directory",
            &rustcodegraph_dir,
            err,
        )
    })?;
    let _ = ensure_gitignore(&rustcodegraph_dir.join(".gitignore"));
    Ok(())
}

pub fn remove_directory(project_root: impl AsRef<Path>) -> Result<(), CodeGraphError> {
    let rustcodegraph_dir = get_code_graph_dir(project_root);
    if !rustcodegraph_dir.exists() {
        return Ok(());
    }

    let metadata = fs::symlink_metadata(&rustcodegraph_dir).map_err(|err| {
        file_error(
            "Failed to inspect RustCodeGraph directory",
            &rustcodegraph_dir,
            err,
        )
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        // uninit 绝不能跟随预置 symlink 删除真实目标；遇到 symlink 或普通文件
        // 只移除路径本身。
        fs::remove_file(&rustcodegraph_dir).map_err(|err| {
            file_error(
                "Failed to remove RustCodeGraph path",
                &rustcodegraph_dir,
                err,
            )
        })?;
        return Ok(());
    }

    fs::remove_dir_all(&rustcodegraph_dir).map_err(|err| {
        file_error(
            "Failed to remove RustCodeGraph directory",
            &rustcodegraph_dir,
            err,
        )
    })
}

pub fn list_directory_contents(
    project_root: impl AsRef<Path>,
) -> Result<Vec<String>, CodeGraphError> {
    let rustcodegraph_dir = get_code_graph_dir(project_root);
    if !rustcodegraph_dir.exists() {
        return Ok(Vec::new());
    }

    let mut files = Vec::new();
    walk_directory_contents(&rustcodegraph_dir, "", &mut files)?;
    Ok(files)
}

pub fn get_directory_size(project_root: impl AsRef<Path>) -> Result<u64, CodeGraphError> {
    let rustcodegraph_dir = get_code_graph_dir(project_root);
    if !rustcodegraph_dir.exists() {
        return Ok(0);
    }
    directory_size(&rustcodegraph_dir)
}

pub fn ensure_subdirectory(
    project_root: impl AsRef<Path>,
    subdir_name: &str,
) -> Result<PathBuf, CodeGraphError> {
    // 子目录名来自内部调用，但仍做路径穿越保护，避免未来把用户输入透传进来。
    if subdir_name.contains("..")
        || subdir_name.contains(std::path::MAIN_SEPARATOR)
        || subdir_name.contains('/')
    {
        return Err(
            ConfigError::new(format!("Invalid subdirectory name: {subdir_name}"), None).into(),
        );
    }

    let subdir_path = get_code_graph_dir(project_root).join(subdir_name);
    if !subdir_path.exists() {
        fs::create_dir_all(&subdir_path).map_err(|err| {
            file_error(
                "Failed to create RustCodeGraph subdirectory",
                &subdir_path,
                err,
            )
        })?;
    }
    Ok(subdir_path)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectoryValidation {
    pub valid: bool,
    pub errors: Vec<String>,
}

pub fn validate_directory(project_root: impl AsRef<Path>) -> DirectoryValidation {
    let mut errors = Vec::new();
    let rustcodegraph_dir = get_code_graph_dir(project_root);

    if !rustcodegraph_dir.exists() {
        errors.push("RustCodeGraph directory does not exist".to_string());
        return DirectoryValidation {
            valid: false,
            errors,
        };
    }

    if !fs::metadata(&rustcodegraph_dir)
        .map(|metadata| metadata.is_dir())
        .unwrap_or(false)
    {
        errors.push(".rustcodegraph exists but is not a directory".to_string());
        return DirectoryValidation {
            valid: false,
            errors,
        };
    }

    let gitignore_path = rustcodegraph_dir.join(".gitignore");
    let existed_before = gitignore_path.exists();
    if !ensure_gitignore(&gitignore_path) && !existed_before {
        errors.push(
            ".gitignore missing in .rustcodegraph directory and could not be created".to_string(),
        );
    }

    DirectoryValidation {
        valid: errors.is_empty(),
        errors,
    }
}

fn walk_directory_contents(
    dir: &Path,
    prefix: &str,
    files: &mut Vec<String>,
) -> Result<(), CodeGraphError> {
    let entries = fs::read_dir(dir)
        .map_err(|err| file_error("Failed to read RustCodeGraph directory", dir, err))?;

    for entry in entries {
        let entry =
            entry.map_err(|err| file_error("Failed to read RustCodeGraph entry", dir, err))?;
        let file_type = entry.file_type().map_err(|err| {
            file_error("Failed to inspect RustCodeGraph entry", &entry.path(), err)
        })?;
        if file_type.is_symlink() {
            // 数据目录可能被用户手工改动；列目录时跳过 symlink，避免状态命令
            // 泄露或遍历项目外路径。
            continue;
        }

        let name = entry.file_name().to_string_lossy().to_string();
        let relative_path = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}/{name}")
        };

        if file_type.is_dir() {
            walk_directory_contents(&entry.path(), &relative_path, files)?;
        } else {
            files.push(relative_path);
        }
    }

    Ok(())
}

fn directory_size(dir: &Path) -> Result<u64, CodeGraphError> {
    let mut total_size = 0;
    let entries = fs::read_dir(dir)
        .map_err(|err| file_error("Failed to read RustCodeGraph directory", dir, err))?;

    for entry in entries {
        let entry =
            entry.map_err(|err| file_error("Failed to read RustCodeGraph entry", dir, err))?;
        let file_type = entry.file_type().map_err(|err| {
            file_error("Failed to inspect RustCodeGraph entry", &entry.path(), err)
        })?;
        if file_type.is_symlink() {
            // 体积统计同样不跟随 symlink，和删除/列目录保持一致的安全边界。
            continue;
        }
        if file_type.is_dir() {
            total_size += directory_size(&entry.path())?;
        } else {
            total_size += entry
                .metadata()
                .map_err(|err| {
                    file_error("Failed to stat RustCodeGraph entry", &entry.path(), err)
                })?
                .len();
        }
    }

    Ok(total_size)
}

fn file_error(message: &str, path: &Path, err: io::Error) -> CodeGraphError {
    FileError::new(message, path.display().to_string(), Some(err.to_string())).into()
}

fn resolve_existing_or_lexical(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| absolutize(path))
}

fn absolutize(path: &Path) -> PathBuf {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    };
    normalize_lexically(&absolute)
}

fn normalize_lexically(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Prefix(_) | Component::RootDir | Component::Normal(_) => {
                normalized.push(component.as_os_str());
            }
        }
    }
    normalized
}

fn is_filesystem_root(path: &Path) -> bool {
    path.parent().is_none()
}

fn home_dir() -> Option<PathBuf> {
    env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .or_else(|| {
            let drive = env::var_os("HOMEDRIVE")?;
            let path = env::var_os("HOMEPATH")?;
            let mut joined = PathBuf::from(drive);
            joined.push(path);
            Some(joined.into_os_string())
        })
        .map(PathBuf::from)
}

fn normalize_for_platform(path: &Path) -> String {
    // macOS 默认文件系统和 Windows 路径比较通常大小写不敏感；Linux 保持
    // 原样，避免把大小写不同的真实路径误判为同一路径。
    let text = path.to_string_lossy().to_string();
    if cfg!(target_os = "macos") || cfg!(windows) {
        text.to_lowercase()
    } else {
        text
    }
}

fn append_separator(path: &str) -> String {
    if path.ends_with(std::path::MAIN_SEPARATOR) {
        path.to_string()
    } else {
        format!("{path}{}", std::path::MAIN_SEPARATOR)
    }
}
