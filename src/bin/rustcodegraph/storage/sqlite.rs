//! CLI storage 路径的 SQLite schema 初始化和连接管理。
//!
//! 这里复用库层 schema.sql；索引写入由库层 CodeGraph 负责。

use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension, params};
use rustcodegraph::db::migrations::{CURRENT_SCHEMA_VERSION, current_time_millis};
use rustcodegraph::directory::get_code_graph_dir;

use super::super::args::CLI_NAME;

pub(crate) fn database_path(project_root: &Path) -> PathBuf {
    // 数据库始终放在项目本地 .rustcodegraph 下，避免不同 workspace 共享状态。
    get_code_graph_dir(project_root).join("rustcodegraph.db")
}

fn ensure_rustcodegraph_dir(project_root: &Path) -> Result<(), String> {
    // .gitignore 随目录创建，防止本地索引文件被误提交。
    let dir = get_code_graph_dir(project_root);
    fs::create_dir_all(&dir).map_err(|err| format!("failed to create {}: {err}", dir.display()))?;
    let gitignore = dir.join(".gitignore");
    if !gitignore.exists() {
        fs::write(
            &gitignore,
            "# RustCodeGraph data files - local to each machine, not for committing.\n*\n!.gitignore\n",
        )
        .map_err(|err| format!("failed to write {}: {err}", gitignore.display()))?;
    }
    Ok(())
}

pub(crate) fn initialize_sqlite_database(
    project_root: &Path,
    replace_invalid: bool,
) -> Result<(), String> {
    ensure_rustcodegraph_dir(project_root)?;
    let path = database_path(project_root);
    if replace_invalid && path.exists() && !sqlite_file_has_schema(&path) {
        // init -i 可修复旧的空文件/非 SQLite 文件；普通 open 不会静默替换用户数据。
        fs::remove_file(&path)
            .map_err(|err| format!("failed to replace invalid {}: {err}", path.display()))?;
    }
    let conn = Connection::open(&path)
        .map_err(|err| format!("failed to open {}: {err}", path.display()))?;
    configure_sqlite(&conn)?;
    conn.execute_batch(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/db/schema.sql"
    )))
    .map_err(|err| format!("failed to initialize schema in {}: {err}", path.display()))?;
    conn.execute(
        "INSERT OR IGNORE INTO schema_versions (version, applied_at, description) VALUES (?1, ?2, ?3)",
        params![
            CURRENT_SCHEMA_VERSION,
            current_time_millis(),
            "Initial schema includes all migrations"
        ],
    )
    .map_err(|err| format!("failed to record schema version in {}: {err}", path.display()))?;
    Ok(())
}

pub(crate) fn open_sqlite_database(project_root: &Path) -> Result<Connection, String> {
    let path = database_path(project_root);
    if !path.exists() {
        return Err(format!(
            "RustCodeGraph not initialized in {}. Run \"{CLI_NAME} init -i\" first.",
            project_root.display()
        ));
    }
    let conn = Connection::open(&path)
        .map_err(|err| format!("failed to open {}: {err}", path.display()))?;
    configure_sqlite(&conn)?;
    if !connection_has_schema(&conn) {
        return Err(format!(
            "RustCodeGraph database at {} is missing schema. Run \"{CLI_NAME} init\" again.",
            path.display()
        ));
    }
    Ok(conn)
}

fn configure_sqlite(conn: &Connection) -> Result<(), String> {
    // WAL + busy_timeout 让 watcher/MCP/CLI 短时间并发读写时更不容易互相打断。
    conn.execute_batch(
        r#"
        PRAGMA busy_timeout = 5000;
        PRAGMA foreign_keys = ON;
        PRAGMA journal_mode = WAL;
        PRAGMA synchronous = NORMAL;
        "#,
    )
    .map_err(|err| format!("failed to configure SQLite: {err}"))
}

fn sqlite_file_has_schema(path: &Path) -> bool {
    let Ok(conn) = Connection::open(path) else {
        return false;
    };
    connection_has_schema(&conn)
}

fn connection_has_schema(conn: &Connection) -> bool {
    conn.query_row(
        "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'nodes' LIMIT 1",
        [],
        |_| Ok(()),
    )
    .optional()
    .ok()
    .flatten()
    .is_some()
}

pub(crate) fn is_sqlite_initialized(project_root: &Path) -> bool {
    // MCP tools/list 依赖这个快速检查决定是否暴露工具；必须同时确认文件和 schema。
    let path = database_path(project_root);
    path.exists() && sqlite_file_has_schema(&path)
}
