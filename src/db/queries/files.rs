use super::*;

// 文件表记录“这个路径上次索引到了什么状态”。同步流程用它比较 hash、
// 找 stale 文件，并决定哪些节点/边需要被替换。
impl<'db> QueryBuilder<'db> {
    pub fn upsert_file(&mut self, file: &FileRecord) -> SqliteResult<()> {
        let stmt = Self::prepare_cached(
            self.db,
            &mut self.stmts.upsert_file,
            r#"
        INSERT INTO files (path, content_hash, language, size, modified_at, indexed_at, node_count, errors)
        VALUES (@path, @contentHash, @language, @size, @modifiedAt, @indexedAt, @nodeCount, @errors)
        ON CONFLICT(path) DO UPDATE SET
          content_hash = @contentHash,
          language = @language,
          size = @size,
          modified_at = @modifiedAt,
          indexed_at = @indexedAt,
          node_count = @nodeCount,
          errors = @errors
      "#,
        )?;
        stmt.run(named_params(vec![
            ("path", SqliteValue::from(file.path.clone())),
            ("contentHash", SqliteValue::from(file.content_hash.clone())),
            (
                "language",
                SqliteValue::from(db_enum_string(&file.language, "language")?),
            ),
            ("size", SqliteValue::from(file.size)),
            ("modifiedAt", SqliteValue::from(file.modified_at)),
            ("indexedAt", SqliteValue::from(file.indexed_at)),
            ("nodeCount", SqliteValue::from(file.node_count)),
            ("errors", json_text_option(file.errors.as_ref(), "errors")?),
        ]))?;
        Ok(())
    }

    pub fn delete_file(&mut self, file_path: &str) -> SqliteResult<()> {
        // nodes 表的删除会通过外键/触发器清理相关边；先删节点再删文件，
        // 可以让增量索引在重写同一路径时不留下旧符号。
        self.delete_nodes_by_file(file_path)?;
        let stmt = Self::prepare_cached(
            self.db,
            &mut self.stmts.delete_file,
            "DELETE FROM files WHERE path = ?",
        )?;
        stmt.run(positional(vec![file_path]))?;
        Ok(())
    }

    pub fn get_file_by_path(&mut self, file_path: &str) -> SqliteResult<Option<FileRecord>> {
        let stmt = Self::prepare_cached(
            self.db,
            &mut self.stmts.get_file_by_path,
            "SELECT * FROM files WHERE path = ?",
        )?;
        stmt.get(positional(vec![file_path]))?
            .map(|row| row_to_file_record(&row))
            .transpose()
    }

    pub fn get_all_files(&mut self) -> SqliteResult<Vec<FileRecord>> {
        let stmt = Self::prepare_cached(
            self.db,
            &mut self.stmts.get_all_files,
            "SELECT * FROM files ORDER BY path",
        )?;
        map_rows(stmt.all(SqliteParams::none())?, row_to_file_record)
    }

    pub fn get_last_indexed_at(&mut self) -> SqliteResult<Option<i64>> {
        let mut stmt = self
            .db
            .prepare("SELECT MAX(indexed_at) AS last FROM files")?;
        Ok(stmt
            .get(SqliteParams::none())?
            .and_then(|row| row_optional_i64(&row, "last")))
    }

    pub fn get_stale_files(
        &mut self,
        current_hashes: &HashMap<String, String>,
    ) -> SqliteResult<Vec<FileRecord>> {
        // 只返回“仍存在但 hash 改变”的文件；已删除文件由扫描差集路径处理，
        // 避免把缺失 hash 误解为内容变化。
        Ok(self
            .get_all_files()?
            .into_iter()
            .filter(|file| {
                current_hashes
                    .get(&file.path)
                    .map(|hash| hash != &file.content_hash)
                    .unwrap_or(false)
            })
            .collect())
    }

    pub fn get_all_file_paths(&mut self) -> SqliteResult<Vec<String>> {
        let stmt = Self::prepare_cached(
            self.db,
            &mut self.stmts.get_all_file_paths,
            "SELECT path FROM files ORDER BY path",
        )?;
        Ok(stmt
            .all(SqliteParams::none())?
            .into_iter()
            .filter_map(|row| row_string(&row, "path").ok())
            .collect())
    }
}
