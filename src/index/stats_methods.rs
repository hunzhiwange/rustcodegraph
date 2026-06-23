//! facade 数据库统计信息。
//!
//! 统计查询用于 CLI/status/MCP 快速展示，因此数据库不可用时返回零值而不是传播错误。

use super::*;

impl CodeGraph {
    pub fn get_stats(&self) -> GraphStats {
        let Ok(conn) = open_facade_database(&self.project_root) else {
            return GraphStats {
                node_count: 0,
                edge_count: 0,
                file_count: 0,
                nodes_by_kind: HashMap::new(),
                edges_by_kind: HashMap::new(),
                files_by_language: HashMap::new(),
                db_size_bytes: 0,
                last_updated: 0,
            };
        };

        let (node_count, edge_count, file_count) = conn
            .query_row(
                r#"
                SELECT
                    (SELECT COUNT(*) FROM nodes) AS node_count,
                    (SELECT COUNT(*) FROM edges) AS edge_count,
                    (SELECT COUNT(*) FROM files) AS file_count
                "#,
                [],
                |row| {
                    Ok((
                        row.get::<_, i64>("node_count")? as u64,
                        row.get::<_, i64>("edge_count")? as u64,
                        row.get::<_, i64>("file_count")? as u64,
                    ))
                },
            )
            .unwrap_or((0, 0, 0));

        let mut nodes_by_kind = HashMap::new();
        if let Ok(mut stmt) =
            conn.prepare("SELECT kind, COUNT(*) AS count FROM nodes GROUP BY kind")
            && let Ok(rows) = stmt.query_map([], |row| {
                Ok((
                    node_kind_from_key(row.get::<_, String>("kind")?),
                    row.get::<_, i64>("count")? as u64,
                ))
            })
        {
            for row in rows.filter_map(Result::ok) {
                nodes_by_kind.insert(row.0, row.1);
            }
        }

        let mut edges_by_kind = HashMap::new();
        if let Ok(mut stmt) =
            conn.prepare("SELECT kind, COUNT(*) AS count FROM edges GROUP BY kind")
            && let Ok(rows) = stmt.query_map([], |row| {
                Ok((
                    edge_kind_from_key(row.get::<_, String>("kind")?),
                    row.get::<_, i64>("count")? as u64,
                ))
            })
        {
            for row in rows.filter_map(Result::ok) {
                edges_by_kind.insert(row.0, row.1);
            }
        }

        let mut files_by_language = HashMap::new();
        if let Ok(mut stmt) =
            conn.prepare("SELECT language, COUNT(*) AS count FROM files GROUP BY language")
            && let Ok(rows) = stmt.query_map([], |row| {
                Ok((
                    facade_language_from_key(row.get::<_, String>("language")?),
                    row.get::<_, i64>("count")? as u64,
                ))
            })
        {
            for row in rows.filter_map(Result::ok) {
                files_by_language.insert(row.0, row.1);
            }
        }

        let last_updated = conn
            .query_row("SELECT MAX(indexed_at) FROM files", [], |row| {
                row.get::<_, Option<TimestampMs>>(0)
            })
            .optional()
            .ok()
            .flatten()
            .flatten()
            .unwrap_or(0);

        GraphStats {
            node_count,
            edge_count,
            file_count,
            nodes_by_kind,
            edges_by_kind,
            files_by_language,
            db_size_bytes: fs::metadata(facade_database_path(&self.project_root))
                .map(|metadata| metadata.len())
                .unwrap_or(0),
            last_updated,
        }
    }

    pub fn get_backend(&self) -> &'static str {
        self.db.get_backend().as_str()
    }

    pub fn get_journal_mode(&mut self) -> String {
        self.db.get_journal_mode().unwrap_or_default()
    }
}
