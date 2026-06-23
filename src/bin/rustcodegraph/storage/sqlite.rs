//! CLI storage 路径的 SQLite schema 管理和批量索引写入。
//!
//! 这里复用库层 schema.sql，但写入数据来自轻量索引器；事务采用整库替换，保证
//! `rustcodegraph index` 完成后不会留下半新半旧的 files/nodes/edges。

use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension, params};
use rustcodegraph::db::migrations::{CURRENT_SCHEMA_VERSION, current_time_millis};
use rustcodegraph::directory::get_code_graph_dir;
use rustcodegraph::types::{Edge, FileRecord, Node};

use super::super::args::CLI_NAME;
use super::json_helpers::{bool_int, enum_string, json_option};

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

pub(crate) fn write_sqlite_index(
    conn: &mut Connection,
    files: &[FileRecord],
    nodes: &[Node],
    edges: &[Edge],
) -> Result<(), String> {
    // 轻量索引是 full replace：先清旧表，再批量插入本次扫描结果，最后一次性 commit。
    let tx = conn
        .transaction()
        .map_err(|err| format!("failed to start index transaction: {err}"))?;
    tx.execute_batch(
        r#"
        DELETE FROM unresolved_refs;
        DELETE FROM edges;
        DELETE FROM nodes;
        DELETE FROM files;
        "#,
    )
    .map_err(|err| format!("failed to clear existing index: {err}"))?;

    {
        // files 先写入，nodes 的 file_path 依赖它，foreign_keys 会保护错误顺序。
        let mut stmt = tx
            .prepare(
                r#"
                INSERT INTO files
                    (path, content_hash, language, size, modified_at, indexed_at, node_count, errors)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?)
                "#,
            )
            .map_err(|err| format!("failed to prepare file insert: {err}"))?;
        for file in files {
            let errors = json_option(file.errors.as_ref())?;
            stmt.execute(params![
                file.path,
                file.content_hash,
                enum_string(&file.language, "language")?,
                file.size as i64,
                file.modified_at,
                file.indexed_at,
                file.node_count as i64,
                errors,
            ])
            .map_err(|err| format!("failed to insert file {}: {err}", file.path))?;
        }
    }

    {
        // Node 字段与库层 schema 保持一致，即使轻量索引器只填其中一部分元数据。
        let mut stmt = tx
            .prepare(
                r#"
                INSERT INTO nodes (
                    id, kind, name, qualified_name, file_path, language,
                    start_line, end_line, start_column, end_column,
                    docstring, signature, visibility,
                    is_exported, is_async, is_static, is_abstract,
                    decorators, type_parameters, return_type, updated_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                "#,
            )
            .map_err(|err| format!("failed to prepare node insert: {err}"))?;
        for node in nodes {
            let visibility = node
                .visibility
                .as_ref()
                .map(|visibility| enum_string(visibility, "visibility"))
                .transpose()?;
            stmt.execute(params![
                node.id,
                enum_string(&node.kind, "kind")?,
                node.name,
                node.qualified_name,
                node.file_path,
                enum_string(&node.language, "language")?,
                node.start_line as i64,
                node.end_line as i64,
                node.start_column as i64,
                node.end_column as i64,
                node.docstring,
                node.signature,
                visibility,
                bool_int(node.is_exported),
                bool_int(node.is_async),
                bool_int(node.is_static),
                bool_int(node.is_abstract),
                json_option(node.decorators.as_ref())?,
                json_option(node.type_parameters.as_ref())?,
                node.return_type,
                node.updated_at,
            ])
            .map_err(|err| format!("failed to insert node {}: {err}", node.id))?;
        }
    }

    {
        // Edge 元数据和 provenance 以 JSON/string 写入，兼容完整索引器产生的启发式边。
        let mut stmt = tx
            .prepare(
                r#"
                INSERT INTO edges (source, target, kind, metadata, line, col, provenance)
                VALUES (?, ?, ?, ?, ?, ?, ?)
                "#,
            )
            .map_err(|err| format!("failed to prepare edge insert: {err}"))?;
        for edge in edges {
            let provenance = edge
                .provenance
                .as_ref()
                .map(|provenance| enum_string(provenance, "provenance"))
                .transpose()?;
            stmt.execute(params![
                edge.source,
                edge.target,
                enum_string(&edge.kind, "kind")?,
                json_option(edge.metadata.as_ref())?,
                edge.line.map(|line| line as i64),
                edge.column.map(|column| column as i64),
                provenance,
            ])
            .map_err(|err| {
                format!(
                    "failed to insert edge {} -> {}: {err}",
                    edge.source, edge.target
                )
            })?;
        }
    }

    tx.commit()
        .map_err(|err| format!("failed to commit index transaction: {err}"))?;
    Ok(())
}
