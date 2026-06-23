//! SQLite adapter facade.
//!
//! This is a direct Rust counterpart to `sqlite-adapter.ts`: a small,
//! better-sqlite3-shaped interface used by the rest of the database layer.
//! The concrete native backend is `rusqlite`, while the public traits keep the
//! query layer shaped like the older better-sqlite3 implementation.
//!
//! 适配器把 rusqlite 包成“prepare/run/get/all/transaction”的窄接口，
//! 让迁移和查询代码不用知道底层 crate，也方便测试替换成占位实现。

use std::cell::RefCell;
use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use rusqlite::types::{ToSqlOutput, Value as RusqliteValue, ValueRef};
use rusqlite::{Connection, ToSql, params_from_iter};

/// The active SQLite backend name surfaced by status/reporting APIs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SqliteBackend {
    /// Name kept for parity with the current TypeScript status shape.
    NodeSqlite,
}

impl SqliteBackend {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NodeSqlite => "node-sqlite",
        }
    }
}

/// Error type for the deferred SQLite abstraction.
#[derive(Debug, Clone)]
pub struct SqliteError {
    pub message: String,
}

impl SqliteError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    pub fn backend_unavailable(db_path: &Path) -> Self {
        Self::new(format!(
            "SQLite backend is not wired yet for {}. Task 02 only translates the facade; a later Cargo/runtime task chooses the concrete backend.",
            db_path.display()
        ))
    }
}

impl fmt::Display for SqliteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl Error for SqliteError {}

pub type SqliteResult<T> = Result<T, SqliteError>;

/// SQLite value used for positional parameters, named parameters, and rows.
#[derive(Debug, Clone, PartialEq)]
pub enum SqliteValue {
    Null,
    Integer(i64),
    Real(f64),
    Text(String),
    Blob(Vec<u8>),
    Bool(bool),
    Json(serde_json::Value),
}

impl SqliteValue {
    pub fn as_i64(&self) -> Option<i64> {
        // SQLite 没有独立 bool 类型；历史数据里布尔值可能以 0/1 或文本保存，
        // 这里统一成 Rust 侧读取语义。
        match self {
            Self::Integer(value) => Some(*value),
            Self::Bool(value) => Some(if *value { 1 } else { 0 }),
            Self::Real(value) => Some(*value as i64),
            Self::Text(value) => value.parse::<i64>().ok(),
            Self::Null | Self::Blob(_) | Self::Json(_) => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Real(value) => Some(*value),
            Self::Integer(value) => Some(*value as f64),
            Self::Bool(value) => Some(if *value { 1.0 } else { 0.0 }),
            Self::Text(value) => value.parse::<f64>().ok(),
            Self::Null | Self::Blob(_) | Self::Json(_) => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(value) => Some(*value),
            Self::Integer(value) => Some(*value != 0),
            Self::Text(value) => match value.as_str() {
                "1" | "true" | "TRUE" => Some(true),
                "0" | "false" | "FALSE" => Some(false),
                _ => None,
            },
            Self::Null | Self::Real(_) | Self::Blob(_) | Self::Json(_) => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Text(value) => Some(value),
            _ => None,
        }
    }

    pub fn into_string_lossy(self) -> Option<String> {
        // 行转换里的可选文本字段允许从数字/bool/JSON 兜底成字符串；
        // blob/null 保持不可读，避免把二进制误展示到 MCP 输出。
        match self {
            Self::Text(value) => Some(value),
            Self::Integer(value) => Some(value.to_string()),
            Self::Real(value) => Some(value.to_string()),
            Self::Bool(value) => Some(if value { "1" } else { "0" }.to_string()),
            Self::Json(value) => Some(value.to_string()),
            Self::Null | Self::Blob(_) => None,
        }
    }
}

impl From<&str> for SqliteValue {
    fn from(value: &str) -> Self {
        Self::Text(value.to_string())
    }
}

impl From<String> for SqliteValue {
    fn from(value: String) -> Self {
        Self::Text(value)
    }
}

impl From<&String> for SqliteValue {
    fn from(value: &String) -> Self {
        Self::Text(value.clone())
    }
}

impl From<i64> for SqliteValue {
    fn from(value: i64) -> Self {
        Self::Integer(value)
    }
}

impl From<i32> for SqliteValue {
    fn from(value: i32) -> Self {
        Self::Integer(value as i64)
    }
}

impl From<usize> for SqliteValue {
    fn from(value: usize) -> Self {
        Self::Integer(value as i64)
    }
}

impl From<u64> for SqliteValue {
    fn from(value: u64) -> Self {
        Self::Integer(value as i64)
    }
}

impl From<bool> for SqliteValue {
    fn from(value: bool) -> Self {
        Self::Integer(if value { 1 } else { 0 })
    }
}

impl From<serde_json::Value> for SqliteValue {
    fn from(value: serde_json::Value) -> Self {
        Self::Json(value)
    }
}

impl<T> From<Option<T>> for SqliteValue
where
    T: Into<SqliteValue>,
{
    fn from(value: Option<T>) -> Self {
        value.map(Into::into).unwrap_or(Self::Null)
    }
}

pub type SqliteRow = HashMap<String, SqliteValue>;

/// Parameters accepted by a prepared statement.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum SqliteParams {
    #[default]
    None,
    Positional(Vec<SqliteValue>),
    Named(HashMap<String, SqliteValue>),
}

impl SqliteParams {
    pub fn none() -> Self {
        Self::None
    }

    pub fn positional(values: Vec<SqliteValue>) -> Self {
        Self::Positional(values)
    }

    pub fn named(values: HashMap<String, SqliteValue>) -> Self {
        Self::Named(values)
    }
}

/// Return shape from statement `run`, matching better-sqlite3.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunResult {
    pub changes: u64,
    pub last_insert_rowid: i64,
}

/// Options for `pragma`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PragmaOptions {
    pub simple: bool,
}

/// Prepared statement abstraction.
pub trait SqliteStatement {
    fn run(&mut self, params: SqliteParams) -> SqliteResult<RunResult>;
    fn get(&mut self, params: SqliteParams) -> SqliteResult<Option<SqliteRow>>;
    fn all(&mut self, params: SqliteParams) -> SqliteResult<Vec<SqliteRow>>;

    /// Lazily yields rows one at a time for unbounded scans.
    fn iterate<'stmt>(
        &'stmt mut self,
        params: SqliteParams,
    ) -> SqliteResult<Box<dyn Iterator<Item = SqliteResult<SqliteRow>> + 'stmt>>;
}

/// Database abstraction used by migrations, connection management, and queries.
pub trait SqliteDatabase {
    fn prepare(&mut self, sql: &str) -> SqliteResult<Box<dyn SqliteStatement>>;
    fn exec(&mut self, sql: &str) -> SqliteResult<()>;
    fn pragma(
        &mut self,
        pragma: &str,
        options: Option<PragmaOptions>,
    ) -> SqliteResult<Option<SqliteValue>>;
    fn transaction(
        &mut self,
        f: &mut dyn FnMut(&mut dyn SqliteDatabase) -> SqliteResult<()>,
    ) -> SqliteResult<()>;
    fn close(&mut self) -> SqliteResult<()>;
    fn is_open(&self) -> bool;
}

/// Return value from `create_database`.
pub struct CreatedDatabase {
    pub db: Box<dyn SqliteDatabase>,
    pub backend: SqliteBackend,
}

/// Open the concrete SQLite backend.
pub fn create_database(db_path: impl AsRef<Path>) -> SqliteResult<CreatedDatabase> {
    // 对外仍返回 trait object，调用方只记录 backend 名称；这样未来换
    // SQLite binding 时不需要重写 query/migration 层。
    let connection = Connection::open(db_path.as_ref()).map_err(sqlite_error)?;
    Ok(CreatedDatabase {
        db: Box::new(RusqliteDatabase::new(connection)),
        backend: SqliteBackend::NodeSqlite,
    })
}

type SharedConnection = Rc<RefCell<Option<Connection>>>;

struct RusqliteDatabase {
    connection: SharedConnection,
}

impl RusqliteDatabase {
    fn new(connection: Connection) -> Self {
        Self {
            connection: Rc::new(RefCell::new(Some(connection))),
        }
    }
}

struct RusqliteStatement {
    connection: SharedConnection,
    sql: String,
}

impl RusqliteStatement {
    fn new(connection: SharedConnection, sql: &str) -> Self {
        Self {
            connection,
            sql: sql.to_string(),
        }
    }
}

impl SqliteDatabase for RusqliteDatabase {
    fn prepare(&mut self, sql: &str) -> SqliteResult<Box<dyn SqliteStatement>> {
        // 先让 rusqlite 编译一次 SQL，提前暴露语法错误；实际执行时再重新
        // prepare，以便 statement 可以独立持有 SQL 而不用借用 connection。
        with_connection(&self.connection, |connection| {
            connection.prepare(sql).map(|_| ()).map_err(sqlite_error)
        })?;
        Ok(Box::new(RusqliteStatement::new(
            Rc::clone(&self.connection),
            sql,
        )))
    }

    fn exec(&mut self, sql: &str) -> SqliteResult<()> {
        with_connection(&self.connection, |connection| {
            connection.execute_batch(sql).map_err(sqlite_error)
        })
    }

    fn pragma(
        &mut self,
        pragma: &str,
        _options: Option<PragmaOptions>,
    ) -> SqliteResult<Option<SqliteValue>> {
        with_connection(&self.connection, |connection| {
            let sql = format!("PRAGMA {pragma}");
            let mut statement = connection.prepare(&sql).map_err(sqlite_error)?;
            let mut rows = statement.query([]).map_err(sqlite_error)?;
            let Some(row) = rows.next().map_err(sqlite_error)? else {
                return Ok(None);
            };
            let value = row.get_ref(0).map_err(sqlite_error)?;
            Ok(Some(value_ref_to_sqlite(value)))
        })
    }

    fn transaction(
        &mut self,
        f: &mut dyn FnMut(&mut dyn SqliteDatabase) -> SqliteResult<()>,
    ) -> SqliteResult<()> {
        // 使用 IMMEDIATE 锁尽早拿到写锁，降低索引写入中途才发现锁冲突的概率。
        // 闭包失败时尽力回滚，保留原始错误给调用方。
        self.exec("BEGIN IMMEDIATE TRANSACTION")?;
        match f(self) {
            Ok(()) => {
                self.exec("COMMIT")?;
                Ok(())
            }
            Err(error) => {
                let _ = self.exec("ROLLBACK");
                Err(error)
            }
        }
    }

    fn close(&mut self) -> SqliteResult<()> {
        let Some(connection) = self.connection.borrow_mut().take() else {
            return Ok(());
        };
        connection
            .close()
            .map_err(|(_connection, error)| sqlite_error(error))
    }

    fn is_open(&self) -> bool {
        self.connection.borrow().is_some()
    }
}

impl SqliteStatement for RusqliteStatement {
    fn run(&mut self, params: SqliteParams) -> SqliteResult<RunResult> {
        with_connection(&self.connection, |connection| {
            let mut statement = connection.prepare(&self.sql).map_err(sqlite_error)?;
            let values = bind_values(&statement, params)?;
            let changes = statement
                .execute(params_from_iter(values.iter()))
                .map_err(sqlite_error)?;
            Ok(RunResult {
                changes: changes as u64,
                last_insert_rowid: connection.last_insert_rowid(),
            })
        })
    }

    fn get(&mut self, params: SqliteParams) -> SqliteResult<Option<SqliteRow>> {
        with_connection(&self.connection, |connection| {
            let mut statement = connection.prepare(&self.sql).map_err(sqlite_error)?;
            let values = bind_values(&statement, params)?;
            let mut rows = statement
                .query(params_from_iter(values.iter()))
                .map_err(sqlite_error)?;
            let Some(row) = rows.next().map_err(sqlite_error)? else {
                return Ok(None);
            };
            row_to_map(row).map(Some)
        })
    }

    fn all(&mut self, params: SqliteParams) -> SqliteResult<Vec<SqliteRow>> {
        with_connection(&self.connection, |connection| {
            let mut statement = connection.prepare(&self.sql).map_err(sqlite_error)?;
            let values = bind_values(&statement, params)?;
            let mut rows = statement
                .query(params_from_iter(values.iter()))
                .map_err(sqlite_error)?;
            let mut output = Vec::new();
            while let Some(row) = rows.next().map_err(sqlite_error)? {
                output.push(row_to_map(row)?);
            }
            Ok(output)
        })
    }

    fn iterate<'stmt>(
        &'stmt mut self,
        params: SqliteParams,
    ) -> SqliteResult<Box<dyn Iterator<Item = SqliteResult<SqliteRow>> + 'stmt>> {
        let rows = self.all(params)?;
        Ok(Box::new(rows.into_iter().map(Ok)))
    }
}

impl ToSql for SqliteValue {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::Owned(sqlite_value_to_rusqlite(self.clone())))
    }
}

fn with_connection<T>(
    connection: &SharedConnection,
    f: impl FnOnce(&Connection) -> SqliteResult<T>,
) -> SqliteResult<T> {
    let borrowed = connection.borrow();
    let connection = borrowed
        .as_ref()
        .ok_or_else(|| SqliteError::new("SQLite connection is closed"))?;
    f(connection)
}

fn sqlite_error(error: rusqlite::Error) -> SqliteError {
    SqliteError::new(error.to_string())
}

fn bind_values(
    statement: &rusqlite::Statement<'_>,
    params: SqliteParams,
) -> SqliteResult<Vec<SqliteValue>> {
    let count = statement.parameter_count();
    match params {
        SqliteParams::None => Ok(Vec::new()),
        SqliteParams::Positional(values) => {
            if values.len() != count {
                return Err(SqliteError::new(format!(
                    "expected {count} SQL parameters, got {}",
                    values.len()
                )));
            }
            Ok(values)
        }
        SqliteParams::Named(values) => {
            // rusqlite 执行时走位置绑定；这里按 statement 的参数顺序把 @name
            // 映射回 Vec，同时兼容调用方传入带前缀或不带前缀的 key。
            let mut output = Vec::with_capacity(count);
            for index in 1..=count {
                let Some(name) = statement.parameter_name(index) else {
                    return Err(SqliteError::new(format!(
                        "SQL parameter {index} is positional but named parameters were provided"
                    )));
                };
                let key = name.trim_start_matches(['@', ':', '$']);
                let value = values
                    .get(key)
                    .or_else(|| values.get(name))
                    .ok_or_else(|| SqliteError::new(format!("missing SQL parameter {name}")))?;
                output.push(value.clone());
            }
            Ok(output)
        }
    }
}

fn row_to_map(row: &rusqlite::Row<'_>) -> SqliteResult<SqliteRow> {
    // 查询层按列名读取，所以这里保留 SQL alias；复杂查询务必给计算列起
    // 稳定别名，否则 row_to_* 会报缺列。
    let statement = row.as_ref();
    let mut output = HashMap::new();
    for index in 0..statement.column_count() {
        let name = statement
            .column_name(index)
            .map_err(sqlite_error)?
            .to_string();
        let value = row.get_ref(index).map_err(sqlite_error)?;
        output.insert(name, value_ref_to_sqlite(value));
    }
    Ok(output)
}

fn sqlite_value_to_rusqlite(value: SqliteValue) -> RusqliteValue {
    match value {
        SqliteValue::Null => RusqliteValue::Null,
        SqliteValue::Integer(value) => RusqliteValue::Integer(value),
        SqliteValue::Real(value) => RusqliteValue::Real(value),
        SqliteValue::Text(value) => RusqliteValue::Text(value),
        SqliteValue::Blob(value) => RusqliteValue::Blob(value),
        SqliteValue::Bool(value) => RusqliteValue::Integer(if value { 1 } else { 0 }),
        SqliteValue::Json(value) => RusqliteValue::Text(value.to_string()),
    }
}

fn value_ref_to_sqlite(value: ValueRef<'_>) -> SqliteValue {
    match value {
        ValueRef::Null => SqliteValue::Null,
        ValueRef::Integer(value) => SqliteValue::Integer(value),
        ValueRef::Real(value) => SqliteValue::Real(value),
        ValueRef::Text(value) => SqliteValue::Text(String::from_utf8_lossy(value).into_owned()),
        ValueRef::Blob(value) => SqliteValue::Blob(value.to_vec()),
    }
}

/// Minimal placeholder database useful for signatures and future tests.
///
/// It is not returned by `create_database`; it remains available for tests that
/// need a closed/unavailable database without touching the filesystem.
///
/// 占位实现故意只报告“后端不可用”，用于验证错误路径和 trait 签名；
/// 生产代码不应通过它执行 SQL。
pub struct DeferredSqliteDatabase {
    path: PathBuf,
    open: bool,
}

impl DeferredSqliteDatabase {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            open: true,
        }
    }
}

impl SqliteDatabase for DeferredSqliteDatabase {
    fn prepare(&mut self, _sql: &str) -> SqliteResult<Box<dyn SqliteStatement>> {
        Err(SqliteError::backend_unavailable(&self.path))
    }

    fn exec(&mut self, _sql: &str) -> SqliteResult<()> {
        Err(SqliteError::backend_unavailable(&self.path))
    }

    fn pragma(
        &mut self,
        _pragma: &str,
        _options: Option<PragmaOptions>,
    ) -> SqliteResult<Option<SqliteValue>> {
        Err(SqliteError::backend_unavailable(&self.path))
    }

    fn transaction(
        &mut self,
        _f: &mut dyn FnMut(&mut dyn SqliteDatabase) -> SqliteResult<()>,
    ) -> SqliteResult<()> {
        Err(SqliteError::backend_unavailable(&self.path))
    }

    fn close(&mut self) -> SqliteResult<()> {
        self.open = false;
        Ok(())
    }

    fn is_open(&self) -> bool {
        self.open
    }
}
