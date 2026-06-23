use super::*;

// 状态查询服务 CLI/MCP 展示，不参与索引逻辑；这里保持 SQL 简单可预期，
// 让调用方能快速判断库规模、最后更新时间和 metadata。
impl<'db> QueryBuilder<'db> {
    pub fn get_node_and_edge_count(&mut self) -> SqliteResult<NodeEdgeCount> {
        let mut stmt = self.db.prepare(
            "SELECT (SELECT COUNT(*) FROM nodes) AS nodes, (SELECT COUNT(*) FROM edges) AS edges",
        )?;
        let row = stmt
            .get(SqliteParams::none())?
            .ok_or_else(|| db_error("node/edge count query returned no row"))?;
        Ok(NodeEdgeCount {
            nodes: row_u64(&row, "nodes")?,
            edges: row_u64(&row, "edges")?,
        })
    }

    pub fn get_stats(&mut self) -> SqliteResult<GraphStats> {
        // 总量和分组统计分开查，避免把三类 GROUP BY 做成交叉连接；
        // db_size_bytes 由更外层根据实际文件系统路径补充。
        let mut count_stmt = self.db.prepare(
            r#"
      SELECT
        (SELECT COUNT(*) FROM nodes) AS node_count,
        (SELECT COUNT(*) FROM edges) AS edge_count,
        (SELECT COUNT(*) FROM files) AS file_count
    "#,
        )?;
        let counts = count_stmt
            .get(SqliteParams::none())?
            .ok_or_else(|| db_error("graph stats count query returned no row"))?;

        let mut nodes_by_kind = HashMap::new();
        let mut stmt = self
            .db
            .prepare("SELECT kind, COUNT(*) as count FROM nodes GROUP BY kind")?;
        for row in stmt.all(SqliteParams::none())? {
            nodes_by_kind.insert(
                parse_db_enum::<NodeKind>(row_string(&row, "kind")?, "kind")?,
                row_u64(&row, "count")?,
            );
        }

        let mut edges_by_kind = HashMap::new();
        let mut stmt = self
            .db
            .prepare("SELECT kind, COUNT(*) as count FROM edges GROUP BY kind")?;
        for row in stmt.all(SqliteParams::none())? {
            edges_by_kind.insert(
                parse_db_enum::<EdgeKind>(row_string(&row, "kind")?, "kind")?,
                row_u64(&row, "count")?,
            );
        }

        let mut files_by_language = HashMap::new();
        let mut stmt = self
            .db
            .prepare("SELECT language, COUNT(*) as count FROM files GROUP BY language")?;
        for row in stmt.all(SqliteParams::none())? {
            files_by_language.insert(
                parse_db_enum::<Language>(row_string(&row, "language")?, "language")?,
                row_u64(&row, "count")?,
            );
        }

        Ok(GraphStats {
            node_count: row_u64(&counts, "node_count")?,
            edge_count: row_u64(&counts, "edge_count")?,
            file_count: row_u64(&counts, "file_count")?,
            nodes_by_kind,
            edges_by_kind,
            files_by_language,
            db_size_bytes: 0,
            last_updated: current_time_millis(),
        })
    }

    pub fn get_metadata(&mut self, key: &str) -> SqliteResult<Option<String>> {
        let mut stmt = self
            .db
            .prepare("SELECT value FROM project_metadata WHERE key = ?")?;
        Ok(stmt
            .get(positional(vec![key]))?
            .and_then(|row| row_optional_string(&row, "value")))
    }

    pub fn set_metadata(&mut self, key: &str, value: &str) -> SqliteResult<()> {
        let mut stmt = self.db.prepare(
            "INSERT INTO project_metadata (key, value, updated_at) VALUES (?, ?, ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
        )?;
        stmt.run(positional(vec![
            SqliteValue::from(key),
            SqliteValue::from(value),
            SqliteValue::from(current_time_millis()),
        ]))?;
        Ok(())
    }

    pub fn get_all_metadata(&mut self) -> SqliteResult<HashMap<String, String>> {
        let mut stmt = self.db.prepare("SELECT key, value FROM project_metadata")?;
        let mut result = HashMap::new();
        for row in stmt.all(SqliteParams::none())? {
            result.insert(row_string(&row, "key")?, row_string(&row, "value")?);
        }
        Ok(result)
    }

    pub fn clear(&mut self) -> SqliteResult<()> {
        self.clear_cache();
        // 清库保留 schema_versions/project_metadata，避免一次重新索引把库
        // 伪装成未迁移状态。
        self.db.transaction(&mut |tx| {
            tx.exec("DELETE FROM unresolved_refs")?;
            tx.exec("DELETE FROM edges")?;
            tx.exec("DELETE FROM nodes")?;
            tx.exec("DELETE FROM files")
        })
    }
}
