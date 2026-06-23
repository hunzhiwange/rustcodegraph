//! Database migrations.
//!
//! This mirrors `migrations.ts`: version 1 is `schema.sql`, and later
//! migrations are applied in ascending version order.
//!
//! 迁移层只负责把旧库推进到当前 schema；初始建表仍由 `schema.sql`
//! 完成，因此这里的版本号从 2 开始，保持和已发布数据库的历史兼容。

use std::time::{SystemTime, UNIX_EPOCH};

use super::sqlite_adapter::{SqliteDatabase, SqliteError, SqliteParams, SqliteResult, SqliteValue};

/// Current schema version.
pub const CURRENT_SCHEMA_VERSION: i64 = 5;

/// Migration definition.
#[derive(Clone, Copy)]
pub struct Migration {
    pub version: i64,
    pub description: &'static str,
    pub up: fn(&mut dyn SqliteDatabase) -> SqliteResult<()>,
}

fn migration_v2(db: &mut dyn SqliteDatabase) -> SqliteResult<()> {
    // v2 是“补齐运行期上下文”的迁移：metadata 供索引状态记录使用，
    // unresolved_refs 的文件/语言字段供后续 resolver 做增量重算，
    // provenance 则让启发式边能和 AST 原生边区分开。
    db.exec(
        r#"
        CREATE TABLE IF NOT EXISTS project_metadata (
          key TEXT PRIMARY KEY,
          value TEXT NOT NULL,
          updated_at INTEGER NOT NULL
        );
        ALTER TABLE unresolved_refs ADD COLUMN file_path TEXT NOT NULL DEFAULT '';
        ALTER TABLE unresolved_refs ADD COLUMN language TEXT NOT NULL DEFAULT 'unknown';
        ALTER TABLE edges ADD COLUMN provenance TEXT DEFAULT NULL;
        CREATE INDEX IF NOT EXISTS idx_unresolved_file_path ON unresolved_refs(file_path);
        CREATE INDEX IF NOT EXISTS idx_edges_provenance ON edges(provenance);
      "#,
    )
}

fn migration_v3(db: &mut dyn SqliteDatabase) -> SqliteResult<()> {
    db.exec(
        r#"
        CREATE INDEX IF NOT EXISTS idx_nodes_lower_name ON nodes(lower(name));
      "#,
    )
}

fn migration_v4(db: &mut dyn SqliteDatabase) -> SqliteResult<()> {
    // source/target 单列索引已经被复合索引覆盖；显式删除可以降低写入成本，
    // 但使用 IF EXISTS 让从任意历史版本升级都保持幂等。
    db.exec(
        r#"
        DROP INDEX IF EXISTS idx_edges_source;
        DROP INDEX IF EXISTS idx_edges_target;
      "#,
    )
}

fn migration_v5(db: &mut dyn SqliteDatabase) -> SqliteResult<()> {
    // return_type 是索引器写入、resolver 读取的轻量类型线索，
    // 保存在节点表里可以避免为接收者类型推断再扫源文件。
    db.exec(
        r#"
        ALTER TABLE nodes ADD COLUMN return_type TEXT;
      "#,
    )
}

/// All migrations in order.
///
/// Version 1 is the initial schema handled by `schema.sql`.
pub fn migrations() -> Vec<Migration> {
    vec![
        Migration {
            version: 2,
            description: "Add project metadata, provenance tracking, and unresolved ref context",
            up: migration_v2,
        },
        Migration {
            version: 3,
            description: "Add lower(name) expression index for memory-efficient case-insensitive lookups",
            up: migration_v3,
        },
        Migration {
            version: 4,
            description: "Drop redundant idx_edges_source / idx_edges_target (covered by source_kind / target_kind composites)",
            up: migration_v4,
        },
        Migration {
            version: 5,
            description: "Add nodes.return_type - normalized return/result type for receiver-type inference (C++ singletons/factories, #645)",
            up: migration_v5,
        },
    ]
}

/// Get the current schema version from the database.
pub fn get_current_version(db: &mut dyn SqliteDatabase) -> SqliteResult<i64> {
    // 老数据库可能还没有 schema_versions 表；把这种情况视为 v0，
    // 由调用方决定是否先套用初始 schema，而不是把初始化路径当成硬错误。
    let mut stmt = match db.prepare("SELECT MAX(version) as version FROM schema_versions") {
        Ok(stmt) => stmt,
        Err(_) => return Ok(0),
    };
    let row = match stmt.get(SqliteParams::none()) {
        Ok(row) => row,
        Err(_) => return Ok(0),
    };
    Ok(row
        .and_then(|mut row| row.remove("version"))
        .and_then(|value| value.as_i64())
        .unwrap_or(0))
}

fn record_migration(
    db: &mut dyn SqliteDatabase,
    version: i64,
    description: &str,
) -> SqliteResult<()> {
    let mut stmt = db.prepare(
        "INSERT INTO schema_versions (version, applied_at, description) VALUES (?, ?, ?)",
    )?;
    stmt.run(SqliteParams::positional(vec![
        SqliteValue::from(version),
        SqliteValue::from(current_time_millis()),
        SqliteValue::from(description),
    ]))?;
    Ok(())
}

/// Run all pending migrations.
pub fn run_migrations(db: &mut dyn SqliteDatabase, from_version: i64) -> SqliteResult<()> {
    let mut pending = migrations()
        .into_iter()
        .filter(|migration| migration.version > from_version)
        .collect::<Vec<_>>();

    if pending.is_empty() {
        return Ok(());
    }

    pending.sort_by_key(|migration| migration.version);

    for migration in pending {
        // 每个迁移和它的历史记录同事务提交，避免“结构已变但版本未记录”
        // 这类半升级状态让下次启动重复执行危险 DDL。
        db.transaction(&mut |tx| {
            (migration.up)(tx)?;
            record_migration(tx, migration.version, migration.description)
        })?;
    }

    Ok(())
}

/// Check if the database needs migration.
pub fn needs_migration(db: &mut dyn SqliteDatabase) -> SqliteResult<bool> {
    Ok(get_current_version(db)? < CURRENT_SCHEMA_VERSION)
}

/// Get list of pending migrations.
pub fn get_pending_migrations(db: &mut dyn SqliteDatabase) -> SqliteResult<Vec<Migration>> {
    let current = get_current_version(db)?;
    let mut pending = migrations()
        .into_iter()
        .filter(|migration| migration.version > current)
        .collect::<Vec<_>>();
    pending.sort_by_key(|migration| migration.version);
    Ok(pending)
}

/// Applied migration history row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationHistoryEntry {
    pub version: i64,
    pub applied_at: i64,
    pub description: Option<String>,
}

/// Get migration history from database.
pub fn get_migration_history(
    db: &mut dyn SqliteDatabase,
) -> SqliteResult<Vec<MigrationHistoryEntry>> {
    let mut stmt = db
        .prepare("SELECT version, applied_at, description FROM schema_versions ORDER BY version")?;
    let rows = stmt.all(SqliteParams::none())?;
    rows.into_iter()
        .map(|mut row| {
            let version = row
                .remove("version")
                .and_then(|value| value.as_i64())
                .ok_or_else(|| SqliteError::new("schema_versions.version was missing"))?;
            let applied_at = row
                .remove("applied_at")
                .and_then(|value| value.as_i64())
                .ok_or_else(|| SqliteError::new("schema_versions.applied_at was missing"))?;
            let description = row
                .remove("description")
                .and_then(SqliteValue::into_string_lossy);
            Ok(MigrationHistoryEntry {
                version,
                applied_at,
                description,
            })
        })
        .collect()
}

pub fn current_time_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}
