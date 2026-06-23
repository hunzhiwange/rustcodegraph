//! CodeGraph error and logging types translated from `errors.ts`.
//!
//! 错误类型在 CLI、库 API 和 MCP 响应之间共用；这里保留可序列化的结构化
//! 上下文，同时实现标准 Error/Display，方便 Rust 调用方按普通错误处理。

use std::collections::HashMap;
use std::env;
use std::error::Error;
use std::fmt;
use std::sync::{Arc, OnceLock, RwLock};

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub type ErrorContext = HashMap<String, Value>;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "name", content = "details", rename_all = "PascalCase")]
pub enum CodeGraphError {
    // 基础变体用于没有专门类别的错误；其余变体保留领域字段，
    // 让 UI/日志能展示文件路径、查询串或数据库操作名。
    CodeGraphError {
        message: String,
        code: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        context: Option<ErrorContext>,
    },
    FileError(FileError),
    ParseError(ParseError),
    DatabaseError(DatabaseError),
    SearchError(SearchError),
    VectorError(VectorError),
    ConfigError(ConfigError),
}

impl CodeGraphError {
    pub fn new(
        message: impl Into<String>,
        code: impl Into<String>,
        context: Option<ErrorContext>,
    ) -> Self {
        Self::CodeGraphError {
            message: message.into(),
            code: code.into(),
            context,
        }
    }

    pub fn message(&self) -> &str {
        match self {
            Self::CodeGraphError { message, .. } => message,
            Self::FileError(error) => &error.message,
            Self::ParseError(error) => &error.message,
            Self::DatabaseError(error) => &error.message,
            Self::SearchError(error) => &error.message,
            Self::VectorError(error) => &error.message,
            Self::ConfigError(error) => &error.message,
        }
    }

    pub fn code(&self) -> &str {
        match self {
            Self::CodeGraphError { code, .. } => code,
            Self::FileError(_) => "FILE_ERROR",
            Self::ParseError(_) => "PARSE_ERROR",
            Self::DatabaseError(_) => "DATABASE_ERROR",
            Self::SearchError(_) => "SEARCH_ERROR",
            Self::VectorError(_) => "VECTOR_ERROR",
            Self::ConfigError(_) => "CONFIG_ERROR",
        }
    }
}

impl fmt::Display for CodeGraphError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message())
    }
}

impl Error for CodeGraphError {}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileError {
    pub message: String,
    pub file_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cause: Option<String>,
}

impl FileError {
    pub fn new(
        message: impl Into<String>,
        file_path: impl Into<String>,
        cause: Option<String>,
    ) -> Self {
        Self {
            message: message.into(),
            file_path: file_path.into(),
            cause,
        }
    }
}

impl From<FileError> for CodeGraphError {
    fn from(error: FileError) -> Self {
        Self::FileError(error)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParseError {
    pub message: String,
    pub file_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cause: Option<String>,
}

impl ParseError {
    pub fn new(
        message: impl Into<String>,
        file_path: impl Into<String>,
        line: Option<u64>,
        column: Option<u64>,
        cause: Option<String>,
    ) -> Self {
        Self {
            message: message.into(),
            file_path: file_path.into(),
            line,
            column,
            cause,
        }
    }
}

impl From<ParseError> for CodeGraphError {
    fn from(error: ParseError) -> Self {
        Self::ParseError(error)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseError {
    pub message: String,
    pub operation: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cause: Option<String>,
}

impl DatabaseError {
    pub fn new(
        message: impl Into<String>,
        operation: impl Into<String>,
        cause: Option<String>,
    ) -> Self {
        Self {
            message: message.into(),
            operation: operation.into(),
            cause,
        }
    }
}

impl From<DatabaseError> for CodeGraphError {
    fn from(error: DatabaseError) -> Self {
        Self::DatabaseError(error)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchError {
    pub message: String,
    pub query: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cause: Option<String>,
}

impl SearchError {
    pub fn new(
        message: impl Into<String>,
        query: impl Into<String>,
        cause: Option<String>,
    ) -> Self {
        Self {
            message: message.into(),
            query: query.into(),
            cause,
        }
    }
}

impl From<SearchError> for CodeGraphError {
    fn from(error: SearchError) -> Self {
        Self::SearchError(error)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VectorError {
    pub message: String,
    pub operation: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cause: Option<String>,
}

impl VectorError {
    pub fn new(
        message: impl Into<String>,
        operation: impl Into<String>,
        cause: Option<String>,
    ) -> Self {
        Self {
            message: message.into(),
            operation: operation.into(),
            cause,
        }
    }
}

impl From<VectorError> for CodeGraphError {
    fn from(error: VectorError) -> Self {
        Self::VectorError(error)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigError {
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<ErrorContext>,
}

impl ConfigError {
    pub fn new(message: impl Into<String>, details: Option<ErrorContext>) -> Self {
        Self {
            message: message.into(),
            details,
        }
    }
}

impl From<ConfigError> for CodeGraphError {
    fn from(error: ConfigError) -> Self {
        Self::ConfigError(error)
    }
}

pub trait Logger: Send + Sync {
    // logger 是全局可替换的轻量接口，测试可以注入 SilentLogger，
    // CLI 默认仍通过 stderr 暴露用户可见警告和错误。
    fn debug(&self, message: &str, context: Option<&ErrorContext>);
    fn warn(&self, message: &str, context: Option<&ErrorContext>);
    fn error(&self, message: &str, context: Option<&ErrorContext>);
}

#[derive(Debug, Clone, Copy)]
pub struct DefaultLogger;

pub const DEFAULT_LOGGER: DefaultLogger = DefaultLogger;

impl Logger for DefaultLogger {
    fn debug(&self, message: &str, context: Option<&ErrorContext>) {
        // debug 默认静默，避免 MCP stdio 被噪音污染；只有显式设置环境变量时
        // 才把诊断信息写到 stderr。
        if env::var_os("RUSTCODEGRAPH_DEBUG").is_some() {
            eprintln!("[RustCodeGraph] {message} {}", format_context(context));
        }
    }

    fn warn(&self, message: &str, context: Option<&ErrorContext>) {
        eprintln!("[RustCodeGraph] {message} {}", format_context(context));
    }

    fn error(&self, message: &str, context: Option<&ErrorContext>) {
        eprintln!("[RustCodeGraph] {message} {}", format_context(context));
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SilentLogger;

pub const SILENT_LOGGER: SilentLogger = SilentLogger;

impl Logger for SilentLogger {
    fn debug(&self, _message: &str, _context: Option<&ErrorContext>) {}
    fn warn(&self, _message: &str, _context: Option<&ErrorContext>) {}
    fn error(&self, _message: &str, _context: Option<&ErrorContext>) {}
}

static CURRENT_LOGGER: OnceLock<RwLock<Arc<dyn Logger>>> = OnceLock::new();

fn current_logger_cell() -> &'static RwLock<Arc<dyn Logger>> {
    // OnceLock + RwLock 让 logger 可以在测试或嵌入式调用中替换，同时读取路径
    // 只 clone Arc，不把锁带到实际日志输出里。
    CURRENT_LOGGER.get_or_init(|| RwLock::new(Arc::new(DEFAULT_LOGGER)))
}

pub fn set_logger<L>(logger: L)
where
    L: Logger + 'static,
{
    let mut current = current_logger_cell()
        .write()
        .expect("current logger lock poisoned");
    *current = Arc::new(logger);
}

pub fn get_logger() -> Arc<dyn Logger> {
    current_logger_cell()
        .read()
        .expect("current logger lock poisoned")
        .clone()
}

pub fn log_debug(message: &str, context: Option<&ErrorContext>) {
    get_logger().debug(message, context);
}

pub fn log_warn(message: &str, context: Option<&ErrorContext>) {
    get_logger().warn(message, context);
}

pub fn log_error(message: &str, context: Option<&ErrorContext>) {
    get_logger().error(message, context);
}

fn format_context(context: Option<&ErrorContext>) -> String {
    match context {
        Some(context) if !context.is_empty() => format!("{context:?}"),
        _ => String::new(),
    }
}
