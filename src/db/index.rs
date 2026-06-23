//! Database layer.
//!
//! Rust counterpart to `index.ts`, preserving connection lifecycle,
//! initialization, migration checks, PRAGMA configuration, and path helpers.
//!
//! 这个模块是数据库层的入口：负责创建/打开项目本地 SQLite 文件、
//! 统一配置连接级 PRAGMA、确保 schema 版本可用，并把底层 adapter
//! 包装成上层索引、查询和 MCP 链路都能复用的连接对象。

use std::fs;
use std::path::{Path, PathBuf};

use crate::directory::get_code_graph_dir;
use crate::types::SchemaVersion;

use super::migrations::{
    CURRENT_SCHEMA_VERSION, current_time_millis, get_current_version, run_migrations,
};
use super::sqlite_adapter::{
    PragmaOptions, SqliteBackend, SqliteDatabase, SqliteError, SqliteParams, SqliteResult,
    SqliteValue, create_database,
};

pub use super::sqlite_adapter::{SqliteBackend as DatabaseBackend, SqliteDatabase as Database};

/// Apply connection-level PRAGMAs.
///
/// `busy_timeout` is intentionally first so later PRAGMAs and the first query
/// wait on a cross-process writer instead of failing immediately.
///
/// 这些设置属于“每个连接都必须重新声明”的 SQLite 状态，不写入 schema。
/// WAL 与 `busy_timeout` 共同支撑 CLI、watcher、MCP daemon 等多进程场景：
/// 有写入者时先等待，而不是让后续查询随机失败。
fn configure_connection(db: &mut dyn SqliteDatabase) -> SqliteResult<()> {
    db.pragma("busy_timeout = 5000", None)?;
    db.pragma("foreign_keys = ON", None)?;
    db.pragma("journal_mode = WAL", None)?;
    db.pragma("synchronous = NORMAL", None)?;
    db.pragma("cache_size = -64000", None)?;
    db.pragma("temp_store = MEMORY", None)?;
    db.pragma("mmap_size = 268435456", None)?;
    Ok(())
}

/// Database connection wrapper with lifecycle management.
///
/// 上层只持有这个包装类型，不直接依赖具体 SQLite 后端。这样 native、
/// 测试替身或未来 wasm 风格 adapter 都能共享同一套初始化与迁移流程。
pub struct DatabaseConnection {
    db: Box<dyn SqliteDatabase>,
    db_path: PathBuf,
    backend: SqliteBackend,
}

impl DatabaseConnection {
    fn new(db: Box<dyn SqliteDatabase>, db_path: PathBuf, backend: SqliteBackend) -> Self {
        Self {
            db,
            db_path,
            backend,
        }
    }

    /// Initialize a new database at the given path.
    ///
    /// 初始化路径会创建父目录、执行完整 schema，然后补写当前 schema 版本。
    /// 对新库不逐条跑历史 migration：`schema.sql` 已经代表最新结构，
    /// 版本记录只用来让后续 `open` 能判断是否需要增量迁移。
    pub fn initialize(db_path: impl AsRef<Path>) -> SqliteResult<Self> {
        let db_path = db_path.as_ref().to_path_buf();
        if let Some(dir) = db_path.parent() {
            fs::create_dir_all(dir).map_err(|error| {
                SqliteError::new(format!("failed to create database dir: {error}"))
            })?;
        }

        let created = create_database(&db_path)?;
        let mut db = created.db;
        configure_connection(db.as_mut())?;

        // schema 仍按源码目录读取，保留早期 TypeScript 迁移过来的文件布局；
        // 是否改成 `include_str!` 嵌入由后续打包任务统一处理。
        let schema_path = Path::new(file!()).with_file_name("schema.sql");
        let schema = fs::read_to_string(&schema_path).map_err(|error| {
            SqliteError::new(format!(
                "failed to read schema at {}: {error}",
                schema_path.display()
            ))
        })?;
        db.exec(&schema)?;

        let current_version = get_current_version(db.as_mut())?;
        if current_version < CURRENT_SCHEMA_VERSION {
            let mut stmt = db.prepare(
                "INSERT OR IGNORE INTO schema_versions (version, applied_at, description) VALUES (?, ?, ?)",
            )?;
            stmt.run(SqliteParams::positional(vec![
                SqliteValue::from(CURRENT_SCHEMA_VERSION),
                SqliteValue::from(current_time_millis()),
                SqliteValue::from("Initial schema includes all migrations"),
            ]))?;
        }

        Ok(Self::new(db, db_path, created.backend))
    }

    /// Open an existing database.
    ///
    /// 打开路径只接受已经存在的库文件。版本落后时在同一条连接上跑迁移，
    /// 避免调用方拿到“打开成功但 schema 还没准备好”的半初始化状态。
    pub fn open(db_path: impl AsRef<Path>) -> SqliteResult<Self> {
        let db_path = db_path.as_ref().to_path_buf();
        if !db_path.exists() {
            return Err(SqliteError::new(format!(
                "Database not found: {}",
                db_path.display()
            )));
        }

        let created = create_database(&db_path)?;
        let mut db = created.db;
        configure_connection(db.as_mut())?;

        let current_version = get_current_version(db.as_mut())?;
        if current_version < CURRENT_SCHEMA_VERSION {
            run_migrations(db.as_mut(), current_version)?;
        }

        Ok(Self::new(db, db_path, created.backend))
    }

    /// Get the underlying database instance.
    pub fn get_db(&mut self) -> &mut dyn SqliteDatabase {
        self.db.as_mut()
    }

    /// Get the SQLite backend serving this connection.
    pub fn get_backend(&self) -> SqliteBackend {
        self.backend
    }

    /// Get database file path.
    pub fn get_path(&self) -> &Path {
        &self.db_path
    }

    /// The journal mode actually in effect (for example, `wal` or `delete`).
    ///
    /// SQLite 可能因为平台、文件系统或只读限制回退到其他 journal mode，
    /// 所以这里查询真实生效值，而不是复述我们前面尝试设置的值。
    pub fn get_journal_mode(&mut self) -> SqliteResult<String> {
        let raw = self
            .db
            .pragma("journal_mode", Some(PragmaOptions { simple: false }))?;
        let mode = match raw {
            Some(SqliteValue::Text(value)) => value,
            Some(value) => value.into_string_lossy().unwrap_or_default(),
            None => String::new(),
        };
        Ok(mode.to_lowercase())
    }

    /// Get current schema version.
    ///
    /// schema_versions 是 append-only 风格的迁移账本；最高版本就是当前库状态。
    /// 行字段缺失会按损坏数据处理，避免静默把坏库当成旧版本继续迁移。
    pub fn get_schema_version(&mut self) -> SqliteResult<Option<SchemaVersion>> {
        let mut stmt = self.db.prepare(
            "SELECT version, applied_at, description FROM schema_versions ORDER BY version DESC LIMIT 1",
        )?;
        let Some(mut row) = stmt.get(SqliteParams::none())? else {
            return Ok(None);
        };

        let version = row
            .remove("version")
            .and_then(|value| value.as_i64())
            .ok_or_else(|| SqliteError::new("schema version row missing version"))?;
        let applied_at = row
            .remove("applied_at")
            .and_then(|value| value.as_i64())
            .ok_or_else(|| SqliteError::new("schema version row missing applied_at"))?;
        let description = row
            .remove("description")
            .and_then(SqliteValue::into_string_lossy);

        Ok(Some(SchemaVersion {
            version: version as u64,
            applied_at,
            description,
        }))
    }

    /// Execute a function within a transaction.
    ///
    /// 底层 adapter 的事务回调不直接返回业务值，因此这里用外层 `Option`
    /// 把闭包结果带出来；如果 adapter 没有执行回调，会显式报错。
    pub fn transaction<T>(
        &mut self,
        mut f: impl FnMut(&mut dyn SqliteDatabase) -> SqliteResult<T>,
    ) -> SqliteResult<T> {
        let mut result = None;
        self.db.transaction(&mut |db| {
            result = Some(f(db)?);
            Ok(())
        })?;
        result.ok_or_else(|| SqliteError::new("transaction callback did not run"))
    }

    /// Get database file size in bytes.
    pub fn get_size(&self) -> SqliteResult<u64> {
        fs::metadata(&self.db_path)
            .map(|metadata| metadata.len())
            .map_err(|error| SqliteError::new(format!("failed to stat database: {error}")))
    }

    /// Optimize database (vacuum and analyze).
    ///
    /// 这是显式维护入口，会执行可能较重的 VACUUM；调用方应只在用户触发
    /// 或可接受阻塞的时机使用。
    pub fn optimize(&mut self) -> SqliteResult<()> {
        self.db.exec("VACUUM")?;
        self.db.exec("ANALYZE")?;
        Ok(())
    }

    /// Lightweight, best-effort maintenance after bulk writes.
    ///
    /// 这里故意吞掉错误：维护失败不应掩盖刚完成的索引写入结果，下一次打开
    /// 或显式优化仍可继续整理统计信息和 WAL。
    pub fn run_maintenance(&mut self) {
        let _ = self.db.exec("PRAGMA optimize");
        let _ = self.db.exec("PRAGMA wal_checkpoint(PASSIVE)");
    }

    /// Close the database connection.
    pub fn close(&mut self) -> SqliteResult<()> {
        self.db.close()
    }

    /// Check if the database connection is open.
    pub fn is_open(&self) -> bool {
        self.db.is_open()
    }
}

/// Default database filename.
pub const DATABASE_FILENAME: &str = "rustcodegraph.db";

/// Get the default database path for a project.
///
/// 所有项目级数据都落在 `.rustcodegraph/` 下，避免污染源码树根目录，
/// 也让 init/uninit、watcher 和 MCP server 对同一位置达成一致。
pub fn get_database_path(project_root: impl AsRef<Path>) -> PathBuf {
    get_code_graph_dir(project_root).join(DATABASE_FILENAME)
}
