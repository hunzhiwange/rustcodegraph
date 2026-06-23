use super::*;

// 节点查询是索引写入和图遍历读取的共同入口；这里维护必填字段兜底、
// 节点缓存失效和批量 id 查询分块，避免这些细节泄漏到上层。
impl<'db> QueryBuilder<'db> {
    /// Insert a new node.
    pub fn insert_node(&mut self, node: &Node) -> SqliteResult<()> {
        self.remove_cached_node(&node.id);
        let stmt = Self::prepare_cached(
            self.db,
            &mut self.stmts.insert_node,
            r#"
        INSERT OR REPLACE INTO nodes (
          id, kind, name, qualified_name, file_path, language,
          start_line, end_line, start_column, end_column,
          docstring, signature, visibility,
          is_exported, is_async, is_static, is_abstract,
          decorators, type_parameters, return_type, updated_at
        ) VALUES (
          @id, @kind, @name, @qualifiedName, @filePath, @language,
          @startLine, @endLine, @startColumn, @endColumn,
          @docstring, @signature, @visibility,
          @isExported, @isAsync, @isStatic, @isAbstract,
          @decorators, @typeParameters, @returnType, @updatedAt
        )
      "#,
        )?;

        if node.id.is_empty() || node.name.is_empty() || node.file_path.is_empty() {
            // 抽取器对不完整语法会尽量产出部分结果；DB 层跳过坏节点，
            // 避免一个空 id 破坏后续边去重或节点替换。
            eprintln!(
                "[RustCodeGraph] Skipping node with missing required fields: {}",
                node.id
            );
            return Ok(());
        }

        stmt.run(named_params(vec![
            ("id", SqliteValue::from(node.id.clone())),
            (
                "kind",
                SqliteValue::from(db_enum_string(&node.kind, "kind")?),
            ),
            ("name", SqliteValue::from(node.name.clone())),
            (
                "qualifiedName",
                SqliteValue::from(if node.qualified_name.is_empty() {
                    node.name.clone()
                } else {
                    node.qualified_name.clone()
                }),
            ),
            ("filePath", SqliteValue::from(node.file_path.clone())),
            (
                "language",
                SqliteValue::from(db_enum_string(&node.language, "language")?),
            ),
            ("startLine", SqliteValue::from(node.start_line)),
            ("endLine", SqliteValue::from(node.end_line)),
            ("startColumn", SqliteValue::from(node.start_column)),
            ("endColumn", SqliteValue::from(node.end_column)),
            ("docstring", SqliteValue::from(node.docstring.clone())),
            ("signature", SqliteValue::from(node.signature.clone())),
            (
                "visibility",
                SqliteValue::from(
                    node.visibility
                        .as_ref()
                        .map(|visibility| db_enum_string(visibility, "visibility"))
                        .transpose()?,
                ),
            ),
            (
                "isExported",
                SqliteValue::from(node.is_exported.unwrap_or(false)),
            ),
            ("isAsync", SqliteValue::from(node.is_async.unwrap_or(false))),
            (
                "isStatic",
                SqliteValue::from(node.is_static.unwrap_or(false)),
            ),
            (
                "isAbstract",
                SqliteValue::from(node.is_abstract.unwrap_or(false)),
            ),
            (
                "decorators",
                json_text_option(node.decorators.as_ref(), "decorators")?,
            ),
            (
                "typeParameters",
                json_text_option(node.type_parameters.as_ref(), "type_parameters")?,
            ),
            ("returnType", SqliteValue::from(node.return_type.clone())),
            (
                "updatedAt",
                SqliteValue::from(if node.updated_at == 0 {
                    current_time_millis()
                } else {
                    node.updated_at
                }),
            ),
        ]))?;
        Ok(())
    }

    /// Insert multiple nodes in a transaction.
    pub fn insert_nodes(&mut self, nodes: &[Node]) -> SqliteResult<()> {
        for node in nodes {
            self.insert_node(node)?;
        }
        Ok(())
    }

    /// Update an existing node.
    pub fn update_node(&mut self, node: &Node) -> SqliteResult<()> {
        self.remove_cached_node(&node.id);
        let stmt = Self::prepare_cached(
            self.db,
            &mut self.stmts.update_node,
            r#"
        UPDATE nodes SET
          kind = @kind,
          name = @name,
          qualified_name = @qualifiedName,
          file_path = @filePath,
          language = @language,
          start_line = @startLine,
          end_line = @endLine,
          start_column = @startColumn,
          end_column = @endColumn,
          docstring = @docstring,
          signature = @signature,
          visibility = @visibility,
          is_exported = @isExported,
          is_async = @isAsync,
          is_static = @isStatic,
          is_abstract = @isAbstract,
          decorators = @decorators,
          type_parameters = @typeParameters,
          return_type = @returnType,
          updated_at = @updatedAt
        WHERE id = @id
      "#,
        )?;

        if node.id.is_empty() || node.name.is_empty() || node.file_path.is_empty() {
            // update 路径和 insert 路径保持同样的宽容度，增量同步才不会因为
            // 单个残缺节点中断整个文件。
            eprintln!(
                "[RustCodeGraph] Skipping node update with missing required fields: {}",
                node.id
            );
            return Ok(());
        }

        stmt.run(named_params(vec![
            ("id", SqliteValue::from(node.id.clone())),
            (
                "kind",
                SqliteValue::from(db_enum_string(&node.kind, "kind")?),
            ),
            ("name", SqliteValue::from(node.name.clone())),
            (
                "qualifiedName",
                SqliteValue::from(if node.qualified_name.is_empty() {
                    node.name.clone()
                } else {
                    node.qualified_name.clone()
                }),
            ),
            ("filePath", SqliteValue::from(node.file_path.clone())),
            (
                "language",
                SqliteValue::from(db_enum_string(&node.language, "language")?),
            ),
            ("startLine", SqliteValue::from(node.start_line)),
            ("endLine", SqliteValue::from(node.end_line)),
            ("startColumn", SqliteValue::from(node.start_column)),
            ("endColumn", SqliteValue::from(node.end_column)),
            ("docstring", SqliteValue::from(node.docstring.clone())),
            ("signature", SqliteValue::from(node.signature.clone())),
            (
                "visibility",
                SqliteValue::from(
                    node.visibility
                        .as_ref()
                        .map(|visibility| db_enum_string(visibility, "visibility"))
                        .transpose()?,
                ),
            ),
            (
                "isExported",
                SqliteValue::from(node.is_exported.unwrap_or(false)),
            ),
            ("isAsync", SqliteValue::from(node.is_async.unwrap_or(false))),
            (
                "isStatic",
                SqliteValue::from(node.is_static.unwrap_or(false)),
            ),
            (
                "isAbstract",
                SqliteValue::from(node.is_abstract.unwrap_or(false)),
            ),
            (
                "decorators",
                json_text_option(node.decorators.as_ref(), "decorators")?,
            ),
            (
                "typeParameters",
                json_text_option(node.type_parameters.as_ref(), "type_parameters")?,
            ),
            ("returnType", SqliteValue::from(node.return_type.clone())),
            (
                "updatedAt",
                SqliteValue::from(if node.updated_at == 0 {
                    current_time_millis()
                } else {
                    node.updated_at
                }),
            ),
        ]))?;
        Ok(())
    }

    /// Delete a node by ID.
    pub fn delete_node(&mut self, id: &str) -> SqliteResult<()> {
        self.remove_cached_node(id);
        let stmt = Self::prepare_cached(
            self.db,
            &mut self.stmts.delete_node,
            "DELETE FROM nodes WHERE id = ?",
        )?;
        stmt.run(positional(vec![id]))?;
        Ok(())
    }

    /// Delete all nodes for a file.
    pub fn delete_nodes_by_file(&mut self, file_path: &str) -> SqliteResult<()> {
        // 缓存里可能有旧文件节点；先按 file_path 扫缓存失效，再让数据库
        // 删除真实记录，保证同一 QueryBuilder 后续读取不会拿到旧符号。
        let ids = self
            .node_cache
            .iter()
            .filter_map(|(id, node)| {
                if node.file_path == file_path {
                    Some(id.clone())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        for id in ids {
            self.remove_cached_node(&id);
        }

        let stmt = Self::prepare_cached(
            self.db,
            &mut self.stmts.delete_nodes_by_file,
            "DELETE FROM nodes WHERE file_path = ?",
        )?;
        stmt.run(positional(vec![file_path]))?;
        Ok(())
    }

    /// Get a node by ID.
    pub fn get_node_by_id(&mut self, id: &str) -> SqliteResult<Option<Node>> {
        if let Some(node) = self.node_cache.get(id).cloned() {
            self.touch_cached_node(id);
            return Ok(Some(node));
        }

        let stmt = Self::prepare_cached(
            self.db,
            &mut self.stmts.get_node_by_id,
            "SELECT * FROM nodes WHERE id = ?",
        )?;
        let Some(row) = stmt.get(positional(vec![id]))? else {
            return Ok(None);
        };
        let node = row_to_node(&row)?;
        self.cache_node(node.clone());
        Ok(Some(node))
    }

    /// Batch lookup: fetch many nodes by ID in chunked IN-list queries.
    pub fn get_nodes_by_ids(&mut self, ids: &[String]) -> SqliteResult<HashMap<String, Node>> {
        let mut out = HashMap::new();
        if ids.is_empty() {
            return Ok(out);
        }

        let mut misses = Vec::new();
        for id in ids {
            // 命中缓存也要 touch，保持批量遍历时的 LRU 顺序接近真实访问顺序。
            if let Some(node) = self.node_cache.get(id).cloned() {
                self.touch_cached_node(id);
                out.insert(id.clone(), node);
            } else {
                misses.push(id.clone());
            }
        }

        for chunk in misses.chunks(SQLITE_PARAM_CHUNK_SIZE) {
            // SQLite 的绑定参数数量有上限；分块后调用方可以安全传入大图遍历
            // 收集到的完整节点集合。
            let sql = format!(
                "SELECT * FROM nodes WHERE id IN ({})",
                placeholders(chunk.len())
            );
            let mut stmt = self.db.prepare(&sql)?;
            let rows = stmt.all(SqliteParams::positional(
                chunk.iter().cloned().map(SqliteValue::from).collect(),
            ))?;
            for row in rows {
                let node = row_to_node(&row)?;
                out.insert(node.id.clone(), node.clone());
                self.cache_node(node);
            }
        }
        Ok(out)
    }

    pub(super) fn get_existing_node_ids(
        &mut self,
        ids: &[String],
    ) -> SqliteResult<HashSet<String>> {
        let mut out = HashSet::new();
        if ids.is_empty() {
            return Ok(out);
        }

        let unique_ids = ids.iter().cloned().collect::<HashSet<_>>();
        let unique_ids = unique_ids.into_iter().collect::<Vec<_>>();
        for chunk in unique_ids.chunks(SQLITE_PARAM_CHUNK_SIZE) {
            // 插边前只需要知道端点是否存在，不需要把节点整行拉回内存。
            let sql = format!(
                "SELECT id FROM nodes WHERE id IN ({})",
                placeholders(chunk.len())
            );
            let mut stmt = self.db.prepare(&sql)?;
            let rows = stmt.all(SqliteParams::positional(
                chunk.iter().cloned().map(SqliteValue::from).collect(),
            ))?;
            for row in rows {
                out.insert(row_string(&row, "id")?);
            }
        }
        Ok(out)
    }

    /// Clear the node cache.
    pub fn clear_cache(&mut self) {
        self.node_cache.clear();
        self.node_cache_order.clear();
    }

    /// Get all nodes in a file.
    pub fn get_nodes_by_file(&mut self, file_path: &str) -> SqliteResult<Vec<Node>> {
        let stmt = Self::prepare_cached(
            self.db,
            &mut self.stmts.get_nodes_by_file,
            "SELECT * FROM nodes WHERE file_path = ? ORDER BY start_line",
        )?;
        map_rows(stmt.all(positional(vec![file_path]))?, row_to_node)
    }

    /// Get all nodes of a specific kind.
    pub fn get_nodes_by_kind(&mut self, kind: NodeKind) -> SqliteResult<Vec<Node>> {
        let stmt = Self::prepare_cached(
            self.db,
            &mut self.stmts.get_nodes_by_kind,
            "SELECT * FROM nodes WHERE kind = ?",
        )?;
        map_rows(
            stmt.all(positional(vec![db_enum_string(&kind, "kind")?]))?,
            row_to_node,
        )
    }

    /// Stream every node of a kind one at a time.
    ///
    /// The concrete backend will own the statement cursor. The facade keeps the
    /// streaming shape so dynamic-edge synthesizers can remain O(1) in row count.
    pub fn iterate_nodes_by_kind(
        &mut self,
        kind: NodeKind,
    ) -> SqliteResult<Box<dyn Iterator<Item = SqliteResult<Node>>>> {
        let mut stmt = self.db.prepare("SELECT * FROM nodes WHERE kind = ?")?;
        let nodes = map_rows(
            stmt.all(positional(vec![db_enum_string(&kind, "kind")?]))?,
            row_to_node,
        )?;
        Ok(Box::new(nodes.into_iter().map(Ok)))
    }

    /// Get all nodes in the database.
    pub fn get_all_nodes(&mut self) -> SqliteResult<Vec<Node>> {
        let mut stmt = self.db.prepare("SELECT * FROM nodes")?;
        map_rows(stmt.all(SqliteParams::none())?, row_to_node)
    }

    /// Get nodes by exact name.
    pub fn get_nodes_by_name(&mut self, name: &str) -> SqliteResult<Vec<Node>> {
        let stmt = Self::prepare_cached(
            self.db,
            &mut self.stmts.get_nodes_by_name,
            "SELECT * FROM nodes WHERE name = ?",
        )?;
        map_rows(stmt.all(positional(vec![name]))?, row_to_node)
    }

    /// Get nodes by exact qualified name.
    pub fn get_nodes_by_qualified_name_exact(
        &mut self,
        qualified_name: &str,
    ) -> SqliteResult<Vec<Node>> {
        let stmt = Self::prepare_cached(
            self.db,
            &mut self.stmts.get_nodes_by_qualified_name_exact,
            "SELECT * FROM nodes WHERE qualified_name = ?",
        )?;
        map_rows(stmt.all(positional(vec![qualified_name]))?, row_to_node)
    }

    /// Get nodes by lower(name).
    pub fn get_nodes_by_lower_name(&mut self, lower_name: &str) -> SqliteResult<Vec<Node>> {
        let stmt = Self::prepare_cached(
            self.db,
            &mut self.stmts.get_nodes_by_lower_name,
            "SELECT * FROM nodes WHERE lower(name) = ?",
        )?;
        map_rows(stmt.all(positional(vec![lower_name]))?, row_to_node)
    }

    pub fn get_all_node_names(&mut self) -> SqliteResult<Vec<String>> {
        let stmt = Self::prepare_cached(
            self.db,
            &mut self.stmts.get_all_node_names,
            "SELECT DISTINCT name FROM nodes",
        )?;
        Ok(stmt
            .all(SqliteParams::none())?
            .into_iter()
            .filter_map(|row| row_string(&row, "name").ok())
            .collect())
    }
}
