use super::*;

// unresolved_refs 是抽取器和 resolver 之间的缓冲区：抽取阶段只记录名字和
// 位置，解析阶段再批量消化并删除已成功解析的引用。
impl<'db> QueryBuilder<'db> {
    pub fn insert_unresolved_ref(&mut self, reference: &UnresolvedReference) -> SqliteResult<()> {
        let stmt = Self::prepare_cached(
            self.db,
            &mut self.stmts.insert_unresolved,
            r#"
        INSERT INTO unresolved_refs (from_node_id, reference_name, reference_kind, line, col, candidates, file_path, language)
        VALUES (@fromNodeId, @referenceName, @referenceKind, @line, @col, @candidates, @filePath, @language)
      "#,
        )?;
        stmt.run(named_params(vec![
            (
                "fromNodeId",
                SqliteValue::from(reference.from_node_id.clone()),
            ),
            (
                "referenceName",
                SqliteValue::from(reference.reference_name.clone()),
            ),
            (
                "referenceKind",
                SqliteValue::from(db_enum_string(&reference.reference_kind, "reference_kind")?),
            ),
            ("line", SqliteValue::from(reference.line)),
            ("col", SqliteValue::from(reference.column)),
            (
                "candidates",
                json_text_option(reference.candidates.as_ref(), "candidates")?,
            ),
            (
                "filePath",
                SqliteValue::from(reference.file_path.clone().unwrap_or_default()),
            ),
            (
                "language",
                SqliteValue::from(
                    reference
                        .language
                        .map(|language| db_enum_string(&language, "language"))
                        .transpose()?
                        .unwrap_or_else(|| "unknown".to_string()),
                ),
            ),
        ]))?;
        Ok(())
    }

    pub fn insert_unresolved_refs_batch(
        &mut self,
        refs: &[UnresolvedReference],
    ) -> SqliteResult<()> {
        if refs.is_empty() {
            return Ok(());
        }
        // 保持逐条写入是为了复用单条路径的 enum/JSON 归一化；批量调用方
        // 通常已经在外层事务里，所以这里不额外包事务。
        for reference in refs {
            self.insert_unresolved_ref(reference)?;
        }
        Ok(())
    }

    pub fn delete_unresolved_by_node(&mut self, node_id: &str) -> SqliteResult<()> {
        let stmt = Self::prepare_cached(
            self.db,
            &mut self.stmts.delete_unresolved_by_node,
            "DELETE FROM unresolved_refs WHERE from_node_id = ?",
        )?;
        stmt.run(positional(vec![node_id]))?;
        Ok(())
    }

    pub fn get_unresolved_by_name(&mut self, name: &str) -> SqliteResult<Vec<UnresolvedReference>> {
        let stmt = Self::prepare_cached(
            self.db,
            &mut self.stmts.get_unresolved_by_name,
            "SELECT * FROM unresolved_refs WHERE reference_name = ?",
        )?;
        map_rows(
            stmt.all(positional(vec![name]))?,
            row_to_unresolved_reference,
        )
    }

    pub fn get_unresolved_references(&mut self) -> SqliteResult<Vec<UnresolvedReference>> {
        let mut stmt = self.db.prepare("SELECT * FROM unresolved_refs")?;
        map_rows(stmt.all(SqliteParams::none())?, row_to_unresolved_reference)
    }

    pub fn get_unresolved_references_count(&mut self) -> SqliteResult<i64> {
        let stmt = Self::prepare_cached(
            self.db,
            &mut self.stmts.get_unresolved_count,
            "SELECT COUNT(*) as count FROM unresolved_refs",
        )?;
        Ok(stmt
            .get(SqliteParams::none())?
            .and_then(|row| row_optional_i64(&row, "count"))
            .unwrap_or(0))
    }

    pub fn get_unresolved_references_batch(
        &mut self,
        offset: i64,
        limit: i64,
    ) -> SqliteResult<Vec<UnresolvedReference>> {
        let stmt = Self::prepare_cached(
            self.db,
            &mut self.stmts.get_unresolved_batch,
            "SELECT * FROM unresolved_refs LIMIT ? OFFSET ?",
        )?;
        map_rows(
            stmt.all(positional(vec![limit, offset]))?,
            row_to_unresolved_reference,
        )
    }

    pub fn get_unresolved_references_by_files(
        &mut self,
        file_paths: &[String],
    ) -> SqliteResult<Vec<UnresolvedReference>> {
        if file_paths.is_empty() {
            return Ok(Vec::new());
        }
        let mut rows = Vec::new();
        for chunk in file_paths.chunks(SQLITE_PARAM_CHUNK_SIZE) {
            // 增量同步按文件集合重解析 unresolved refs；分块避免大变更集
            // 撞上 SQLite 参数数量限制。
            let sql = format!(
                "SELECT * FROM unresolved_refs WHERE file_path IN ({})",
                placeholders(chunk.len())
            );
            let mut stmt = self.db.prepare(&sql)?;
            rows.extend(stmt.all(SqliteParams::positional(
                chunk.iter().cloned().map(SqliteValue::from).collect(),
            ))?);
        }
        map_rows(rows, row_to_unresolved_reference)
    }

    pub fn clear_unresolved_references(&mut self) -> SqliteResult<()> {
        self.db.exec("DELETE FROM unresolved_refs")
    }

    pub fn delete_resolved_references(&mut self, from_node_ids: &[String]) -> SqliteResult<()> {
        if from_node_ids.is_empty() {
            return Ok(());
        }
        // 这里按 from_node_id 批量删除“该节点所有已尝试引用”，用于粗粒度
        // 解析成功路径；精确删除见 delete_specific_resolved_references。
        let sql = format!(
            "DELETE FROM unresolved_refs WHERE from_node_id IN ({})",
            placeholders(from_node_ids.len())
        );
        let mut stmt = self.db.prepare(&sql)?;
        stmt.run(SqliteParams::positional(
            from_node_ids
                .iter()
                .cloned()
                .map(SqliteValue::from)
                .collect(),
        ))?;
        Ok(())
    }

    pub fn delete_specific_resolved_references(
        &mut self,
        refs: &[ResolvedReferenceKey],
    ) -> SqliteResult<()> {
        if refs.is_empty() {
            return Ok(());
        }
        // 有些 resolver 只确认同一节点的部分引用，必须按三元组删除，
        // 保留未解析的同名/不同 kind 引用给后续策略继续尝试。
        let mut stmt = self.db.prepare(
            "DELETE FROM unresolved_refs WHERE from_node_id = ? AND reference_name = ? AND reference_kind = ?",
        )?;
        self.db.transaction(&mut |_tx| {
            for reference in refs {
                stmt.run(positional(vec![
                    reference.from_node_id.as_str(),
                    reference.reference_name.as_str(),
                    reference.reference_kind.as_str(),
                ]))?;
            }
            Ok(())
        })
    }
}
