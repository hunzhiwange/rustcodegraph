use super::*;

// 边查询集中处理调用图/引用图的写入与邻接读取；resolver 和 MCP 图遍历
// 都依赖这些入口保持“缺失端点不写入、读取时按需过滤”的语义。
impl<'db> QueryBuilder<'db> {
    /// Insert a new edge.
    pub fn insert_edge(&mut self, edge: &Edge) -> SqliteResult<()> {
        let stmt = Self::prepare_cached(
            self.db,
            &mut self.stmts.insert_edge,
            r#"
        INSERT OR IGNORE INTO edges (source, target, kind, metadata, line, col, provenance)
        VALUES (@source, @target, @kind, @metadata, @line, @col, @provenance)
      "#,
        )?;
        stmt.run(named_params(vec![
            ("source", SqliteValue::from(edge.source.clone())),
            ("target", SqliteValue::from(edge.target.clone())),
            (
                "kind",
                SqliteValue::from(db_enum_string(&edge.kind, "kind")?),
            ),
            (
                "metadata",
                json_text_option(edge.metadata.as_ref(), "metadata")?,
            ),
            ("line", SqliteValue::from(edge.line)),
            ("col", SqliteValue::from(edge.column)),
            (
                "provenance",
                SqliteValue::from(
                    edge.provenance
                        .as_ref()
                        .map(|provenance| db_enum_string(provenance, "provenance"))
                        .transpose()?,
                ),
            ),
        ]))?;
        Ok(())
    }

    /// Insert multiple edges in a transaction, skipping dangling endpoints.
    pub fn insert_edges(&mut self, edges: &[Edge]) -> SqliteResult<()> {
        if edges.is_empty() {
            return Ok(());
        }
        let mut endpoint_ids = HashSet::new();
        for edge in edges {
            endpoint_ids.insert(edge.source.clone());
            endpoint_ids.insert(edge.target.clone());
        }
        let endpoint_ids = endpoint_ids.into_iter().collect::<Vec<_>>();
        let existing_node_ids = self.get_existing_node_ids(&endpoint_ids)?;
        for edge in edges {
            // 增量索引期间可能先删除旧节点再写新边；跳过悬空端点比写入坏边
            // 更安全，后续 resolver 可在下一轮重建时补齐。
            if existing_node_ids.contains(&edge.source) && existing_node_ids.contains(&edge.target)
            {
                self.insert_edge(edge)?;
            }
        }
        Ok(())
    }

    pub fn delete_edges_by_source(&mut self, source_id: &str) -> SqliteResult<()> {
        let stmt = Self::prepare_cached(
            self.db,
            &mut self.stmts.delete_edges_by_source,
            "DELETE FROM edges WHERE source = ?",
        )?;
        stmt.run(positional(vec![source_id]))?;
        Ok(())
    }

    pub fn get_outgoing_edges(
        &mut self,
        source_id: &str,
        kinds: Option<Vec<EdgeKind>>,
        provenance: Option<&str>,
    ) -> SqliteResult<Vec<Edge>> {
        // 常见无过滤路径使用缓存语句；带 kind/provenance 的路径需要动态
        // IN 列表，所以单独 prepare。
        if kinds.as_ref().map(|k| !k.is_empty()).unwrap_or(false) || provenance.is_some() {
            let mut sql = "SELECT * FROM edges WHERE source = ?".to_string();
            let mut params = vec![SqliteValue::from(source_id)];
            if let Some(kinds) = &kinds {
                sql.push_str(&format!(" AND kind IN ({})", placeholders(kinds.len())));
                params.extend(
                    kinds
                        .iter()
                        .map(|kind| db_enum_string(kind, "kind").map(SqliteValue::from))
                        .collect::<SqliteResult<Vec<_>>>()?,
                );
            }
            if let Some(provenance) = provenance {
                sql.push_str(" AND provenance = ?");
                params.push(SqliteValue::from(provenance));
            }
            let mut stmt = self.db.prepare(&sql)?;
            return map_rows(stmt.all(SqliteParams::positional(params))?, row_to_edge);
        }

        let stmt = Self::prepare_cached(
            self.db,
            &mut self.stmts.get_edges_by_source,
            "SELECT * FROM edges WHERE source = ?",
        )?;
        map_rows(stmt.all(positional(vec![source_id]))?, row_to_edge)
    }

    pub fn get_incoming_edges(
        &mut self,
        target_id: &str,
        kinds: Option<Vec<EdgeKind>>,
    ) -> SqliteResult<Vec<Edge>> {
        if let Some(kinds) = kinds.filter(|kinds| !kinds.is_empty()) {
            let sql = format!(
                "SELECT * FROM edges WHERE target = ? AND kind IN ({})",
                placeholders(kinds.len())
            );
            let mut params = vec![SqliteValue::from(target_id)];
            params.extend(
                kinds
                    .iter()
                    .map(|kind| db_enum_string(kind, "kind").map(SqliteValue::from))
                    .collect::<SqliteResult<Vec<_>>>()?,
            );
            let mut stmt = self.db.prepare(&sql)?;
            return map_rows(stmt.all(SqliteParams::positional(params))?, row_to_edge);
        }

        let stmt = Self::prepare_cached(
            self.db,
            &mut self.stmts.get_edges_by_target,
            "SELECT * FROM edges WHERE target = ?",
        )?;
        map_rows(stmt.all(positional(vec![target_id]))?, row_to_edge)
    }

    pub fn find_edges_between_nodes(
        &mut self,
        node_ids: &[String],
        kinds: Option<Vec<EdgeKind>>,
    ) -> SqliteResult<Vec<Edge>> {
        if node_ids.is_empty() {
            return Ok(Vec::new());
        }
        let ids_json = serde_json::to_string(node_ids)
            .map_err(|error| db_error(format!("failed to serialize node ids: {error}")))?;
        // 用 json_each 承载节点集合，避免为“子图内部边”查询生成超长 IN
        // 列表；这条路径通常服务 explore-flow 的上下文收缩。
        let mut sql = "SELECT * FROM edges WHERE source IN (SELECT value FROM json_each(?)) AND target IN (SELECT value FROM json_each(?))".to_string();
        let mut params = vec![
            SqliteValue::from(ids_json.clone()),
            SqliteValue::from(ids_json),
        ];
        if let Some(kinds) = &kinds {
            sql.push_str(&format!(" AND kind IN ({})", placeholders(kinds.len())));
            params.extend(
                kinds
                    .iter()
                    .map(|kind| db_enum_string(kind, "kind").map(SqliteValue::from))
                    .collect::<SqliteResult<Vec<_>>>()?,
            );
        }
        let mut stmt = self.db.prepare(&sql)?;
        map_rows(stmt.all(SqliteParams::positional(params))?, row_to_edge)
    }

    pub fn get_dependent_file_paths(&mut self, file_path: &str) -> SqliteResult<Vec<String>> {
        let mut stmt = self.db.prepare(
            r#"SELECT DISTINCT src.file_path AS fp
      FROM edges e
      JOIN nodes tgt ON tgt.id = e.target
      JOIN nodes src ON src.id = e.source
      WHERE tgt.file_path = ?
        AND e.kind != 'contains'
        AND src.file_path != ?"#,
        )?;
        Ok(stmt
            .all(positional(vec![file_path, file_path]))?
            .into_iter()
            .filter_map(|row| row_string(&row, "fp").ok())
            .collect())
    }

    pub fn get_dependency_file_paths(&mut self, file_path: &str) -> SqliteResult<Vec<String>> {
        let mut stmt = self.db.prepare(
            r#"SELECT DISTINCT tgt.file_path AS fp
      FROM edges e
      JOIN nodes src ON src.id = e.source
      JOIN nodes tgt ON tgt.id = e.target
      WHERE src.file_path = ?
        AND e.kind != 'contains'
        AND tgt.file_path != ?"#,
        )?;
        Ok(stmt
            .all(positional(vec![file_path, file_path]))?
            .into_iter()
            .filter_map(|row| row_string(&row, "fp").ok())
            .collect())
    }
}
