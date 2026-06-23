use super::*;

// facade 数据库 helper 负责把 public API 的类型和 SQLite schema 互相转换。
// 失败时多数查询返回空集合，初始化/索引路径才把错误提升为 CodeGraphError。
pub(super) fn facade_database_path(project_root: &Path) -> PathBuf {
    get_code_graph_dir(project_root).join("rustcodegraph.db")
}

pub(super) fn database_error(operation: &str, err: impl std::fmt::Display) -> CodeGraphError {
    DatabaseError::new(
        format!("Failed to {operation} RustCodeGraph database"),
        operation,
        Some(err.to_string()),
    )
    .into()
}

pub(super) fn configure_facade_sqlite(conn: &Connection) -> Result<(), String> {
    // WAL + busy_timeout 让 watch/sync 与查询并发时更稳；foreign_keys 必须显式开启。
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

pub(super) fn initialize_facade_database(project_root: &Path) -> Result<(), String> {
    let db_path = facade_database_path(project_root);
    if let Some(parent) = db_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create {}: {err}", parent.display()))?;
    }
    if db_path.exists() {
        return Ok(());
    }

    let conn = Connection::open(&db_path)
        .map_err(|err| format!("failed to open {}: {err}", db_path.display()))?;
    configure_facade_sqlite(&conn)?;
    conn.execute_batch(include_str!("../db/schema.sql"))
        .map_err(|err| {
            format!(
                "failed to initialize schema in {}: {err}",
                db_path.display()
            )
        })?;
    Ok(())
}

pub(super) fn open_facade_database(project_root: &Path) -> Result<Connection, String> {
    let db_path = facade_database_path(project_root);
    let conn = Connection::open(&db_path)
        .map_err(|err| format!("failed to open {}: {err}", db_path.display()))?;
    configure_facade_sqlite(&conn)?;
    Ok(conn)
}

pub(super) fn ensure_facade_database(project_root: &Path) -> Result<Connection, String> {
    if !facade_database_path(project_root).exists() {
        initialize_facade_database(project_root)?;
    }
    open_facade_database(project_root)
}

pub(super) fn read_facade_last_indexed_at(
    conn: &Connection,
) -> Result<Option<TimestampMs>, String> {
    conn.query_row("SELECT MAX(indexed_at) FROM files", [], |row| {
        row.get::<_, Option<TimestampMs>>(0)
    })
    .optional()
    .map(|value| value.flatten())
    .map_err(|err| format!("failed to read last indexed timestamp: {err}"))
}

pub(super) fn read_facade_metadata(conn: &Connection, key: &str) -> Result<Option<String>, String> {
    conn.query_row(
        "SELECT value FROM project_metadata WHERE key = ?",
        [key],
        |row| row.get::<_, String>(0),
    )
    .optional()
    .map_err(|err| format!("failed to read metadata `{key}`: {err}"))
}

pub(super) fn read_facade_index_build_info(project_root: &Path) -> Option<IndexBuildInfo> {
    // build info 是展示/状态用途，读取失败不应阻止项目打开。
    let conn = open_facade_database(project_root).ok()?;
    let version = read_facade_metadata(&conn, "indexed_with_version")
        .ok()
        .flatten();
    let extraction_version = read_facade_metadata(&conn, "indexed_with_extraction_version")
        .ok()
        .flatten()
        .and_then(|value| value.parse::<u64>().ok());
    Some(IndexBuildInfo {
        version,
        extraction_version,
    })
}
pub(super) fn json_string_option<T: Serialize>(value: Option<&T>) -> Option<String> {
    value.and_then(|value| serde_json::to_string(value).ok())
}

pub(super) fn bool_int(value: Option<bool>) -> i64 {
    i64::from(value.unwrap_or(false))
}

pub(super) fn kind_key(kind: NodeKind) -> &'static str {
    match kind {
        NodeKind::File => "file",
        NodeKind::Module => "module",
        NodeKind::Class => "class",
        NodeKind::Struct => "struct",
        NodeKind::Interface => "interface",
        NodeKind::Trait => "trait",
        NodeKind::Protocol => "protocol",
        NodeKind::Function => "function",
        NodeKind::Method => "method",
        NodeKind::Property => "property",
        NodeKind::Field => "field",
        NodeKind::Variable => "variable",
        NodeKind::Constant => "constant",
        NodeKind::Enum => "enum",
        NodeKind::EnumMember => "enum_member",
        NodeKind::TypeAlias => "type_alias",
        NodeKind::Namespace => "namespace",
        NodeKind::Parameter => "parameter",
        NodeKind::Import => "import",
        NodeKind::Export => "export",
        NodeKind::Route => "route",
        NodeKind::Component => "component",
    }
}

pub(super) fn edge_kind_key(kind: EdgeKind) -> &'static str {
    match kind {
        EdgeKind::Contains => "contains",
        EdgeKind::Calls => "calls",
        EdgeKind::Imports => "imports",
        EdgeKind::Exports => "exports",
        EdgeKind::Extends => "extends",
        EdgeKind::Implements => "implements",
        EdgeKind::References => "references",
        EdgeKind::TypeOf => "type_of",
        EdgeKind::Returns => "returns",
        EdgeKind::Instantiates => "instantiates",
        EdgeKind::Overrides => "overrides",
        EdgeKind::Decorates => "decorates",
    }
}

pub(super) fn edge_provenance_key(provenance: EdgeProvenance) -> &'static str {
    match provenance {
        EdgeProvenance::TreeSitter => "tree-sitter",
        EdgeProvenance::Scip => "scip",
        EdgeProvenance::Heuristic => "heuristic",
    }
}

pub(super) fn edge_provenance_from_key(key: String) -> Option<EdgeProvenance> {
    match key.as_str() {
        "tree-sitter" => Some(EdgeProvenance::TreeSitter),
        "scip" => Some(EdgeProvenance::Scip),
        "heuristic" => Some(EdgeProvenance::Heuristic),
        _ => None,
    }
}

pub(super) fn reference_kind_key(kind: ReferenceKind) -> &'static str {
    match kind {
        ReferenceKind::Contains => "contains",
        ReferenceKind::Calls => "calls",
        ReferenceKind::Imports => "imports",
        ReferenceKind::Exports => "exports",
        ReferenceKind::Extends => "extends",
        ReferenceKind::Implements => "implements",
        ReferenceKind::References => "references",
        ReferenceKind::TypeOf => "type_of",
        ReferenceKind::Returns => "returns",
        ReferenceKind::Instantiates => "instantiates",
        ReferenceKind::Overrides => "overrides",
        ReferenceKind::Decorates => "decorates",
        ReferenceKind::FunctionRef => "function_ref",
    }
}

pub(super) fn visibility_key(visibility: Visibility) -> &'static str {
    match visibility {
        Visibility::Public => "public",
        Visibility::Private => "private",
        Visibility::Protected => "protected",
        Visibility::Internal => "internal",
    }
}

pub(super) fn query_facade_nodes<const N: usize>(
    conn: &Connection,
    sql: &str,
    params: [&str; N],
) -> Vec<Node> {
    // 查询类 facade 方法偏向“尽力返回”，避免单条 SQL 失败让整个 SDK 调用 panic。
    let Ok(mut stmt) = conn.prepare(sql) else {
        return Vec::new();
    };
    stmt.query_map(params_from_iter(params), row_to_facade_node)
        .map(|rows| rows.filter_map(Result::ok).collect())
        .unwrap_or_default()
}

pub(super) fn query_facade_edges<const N: usize>(
    conn: &Connection,
    sql: &str,
    params: [&str; N],
) -> Vec<Edge> {
    let Ok(mut stmt) = conn.prepare(sql) else {
        return Vec::new();
    };
    stmt.query_map(params_from_iter(params), row_to_facade_edge)
        .map(|rows| rows.filter_map(Result::ok).collect())
        .unwrap_or_default()
}

pub(super) fn row_to_facade_node(row: &rusqlite::Row<'_>) -> rusqlite::Result<Node> {
    // DB 中枚举以稳定字符串保存，读出时通过 serde 回到 Rust enum；未知值采用
    // 保守默认，兼容未来 schema/枚举扩展。
    Ok(Node {
        id: row.get("id")?,
        kind: node_kind_from_key(row.get::<_, String>("kind")?),
        name: row.get("name")?,
        qualified_name: row.get("qualified_name")?,
        file_path: row.get("file_path")?,
        language: facade_language_from_key(row.get::<_, String>("language")?),
        start_line: row.get::<_, i64>("start_line")? as u64,
        end_line: row.get::<_, i64>("end_line")? as u64,
        start_column: row.get::<_, i64>("start_column")? as u64,
        end_column: row.get::<_, i64>("end_column")? as u64,
        docstring: row.get("docstring")?,
        signature: row.get("signature")?,
        visibility: row
            .get::<_, Option<String>>("visibility")?
            .and_then(facade_visibility_from_key),
        is_exported: Some(row.get::<_, i64>("is_exported")? != 0),
        is_async: Some(row.get::<_, i64>("is_async")? != 0),
        is_static: Some(row.get::<_, i64>("is_static")? != 0),
        is_abstract: Some(row.get::<_, i64>("is_abstract")? != 0),
        decorators: json_option(row.get::<_, Option<String>>("decorators")?),
        type_parameters: json_option(row.get::<_, Option<String>>("type_parameters")?),
        return_type: row.get("return_type")?,
        updated_at: row.get("updated_at")?,
    })
}

pub(super) fn row_to_facade_edge(row: &rusqlite::Row<'_>) -> rusqlite::Result<Edge> {
    Ok(Edge {
        source: row.get("source")?,
        target: row.get("target")?,
        kind: edge_kind_from_key(row.get::<_, String>("kind")?),
        metadata: json_option(row.get::<_, Option<String>>("metadata")?),
        line: row.get::<_, Option<i64>>("line")?.map(|value| value as u64),
        column: row.get::<_, Option<i64>>("col")?.map(|value| value as u64),
        provenance: row
            .get::<_, Option<String>>("provenance")?
            .and_then(edge_provenance_from_key),
    })
}

pub(super) fn row_to_facade_file(row: &rusqlite::Row<'_>) -> rusqlite::Result<FileRecord> {
    Ok(FileRecord {
        path: row.get("path")?,
        content_hash: row.get("content_hash")?,
        language: facade_language_from_key(row.get::<_, String>("language")?),
        size: row.get::<_, i64>("size")? as u64,
        modified_at: row.get("modified_at")?,
        indexed_at: row.get("indexed_at")?,
        node_count: row.get::<_, i64>("node_count")? as u64,
        errors: json_option(row.get::<_, Option<String>>("errors")?),
    })
}

pub(super) fn json_option<T: serde::de::DeserializeOwned>(raw: Option<String>) -> Option<T> {
    raw.and_then(|raw| serde_json::from_str(&raw).ok())
}

pub(super) fn facade_language_from_key(value: String) -> Language {
    serde_json::from_value(Value::String(value)).unwrap_or(Language::Unknown)
}

pub(super) fn facade_visibility_from_key(value: String) -> Option<Visibility> {
    serde_json::from_value(Value::String(value)).ok()
}

pub(super) fn node_kind_from_key(value: String) -> NodeKind {
    serde_json::from_value(Value::String(value)).unwrap_or(NodeKind::Variable)
}

pub(super) fn edge_kind_from_key(value: String) -> EdgeKind {
    serde_json::from_value(Value::String(value)).unwrap_or(EdgeKind::References)
}

pub(super) fn index_failure(message: String, started: Instant) -> IndexResult {
    IndexResult {
        success: false,
        files_indexed: 0,
        files_skipped: 0,
        files_errored: 1,
        nodes_created: 0,
        edges_created: 0,
        errors: vec![ExtractionError {
            message,
            file_path: None,
            line: None,
            column: None,
            severity: ExtractionSeverity::Error,
            code: Some("index_error".to_owned()),
        }],
        duration_ms: started.elapsed().as_millis() as u64,
    }
}

pub(super) fn now_ms() -> TimestampMs {
    system_time_ms(SystemTime::now())
}

pub(super) fn system_time_ms(time: SystemTime) -> TimestampMs {
    time.duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as TimestampMs)
        .unwrap_or(0)
}

pub(super) fn existing_source_files(project_root: &Path) -> Vec<String> {
    scan_directory(project_root, None)
        .into_iter()
        .map(|path| normalize_project_path(&path))
        .filter(|path| project_root.join(path).is_file())
        .collect()
}

pub(super) fn normalize_project_path(path: &str) -> String {
    path.replace('\\', "/").trim_start_matches("./").to_owned()
}
