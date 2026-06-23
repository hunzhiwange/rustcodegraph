use crate::db::queries::QueryBuilder;
use crate::types::{ExtractionError, ExtractionSeverity, FileRecord};
use sha2::{Digest, Sha256};

/// 生成用于增量同步的稳定内容指纹；调用方只比较哈希，不需要保留整份源码。
pub fn hash_content(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub(super) fn extraction_error(
    message: String,
    file_path: Option<String>,
    code: &str,
) -> ExtractionError {
    ExtractionError {
        message,
        file_path,
        line: None,
        column: None,
        severity: ExtractionSeverity::Error,
        code: Some(code.to_owned()),
    }
}

pub(super) fn normalize_slashes(path: &str) -> String {
    path.replace('\\', "/")
}

pub(super) fn pattern_matches(pattern: &str, path: &str) -> bool {
    let pattern = normalize_slashes(pattern)
        .trim_start_matches('/')
        .to_owned();
    let path = normalize_slashes(path).trim_start_matches("./").to_owned();
    if let Some(prefix) = pattern.strip_suffix("*/") {
        return path.contains(prefix.trim_end_matches('*'));
    }
    // 这里不是完整 gitignore 解析器，只覆盖默认规则和常见后缀/目录通配；
    // 真正的 Git 仓库扫描优先交给 git ls-files 处理。
    if let Some(suffix) = pattern.strip_prefix('*') {
        return path.ends_with(suffix.trim_end_matches('/'));
    }
    let pattern = pattern.trim_end_matches('/');
    path == pattern
        || path.starts_with(&format!("{pattern}/"))
        || path.contains(&format!("/{pattern}/"))
}

pub(super) fn tracked_files_placeholder(_queries: &QueryBuilder<'_>) -> Vec<FileRecord> {
    // 未来接入 QueryBuilder 的已跟踪文件查询；目前让非 Git 增量同步走“全新增”保守路径。
    Vec::new()
}
