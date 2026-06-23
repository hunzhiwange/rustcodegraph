use super::*;

// 这些查询不是通用图 API，而是给 MCP 初始化/提示上下文寻找“项目入口”
// 的启发式摘要。宁可返回 None，也不要把测试或生成文件误报成主入口。
impl<'db> QueryBuilder<'db> {
    /// Find the file with the densest internal call graph.
    pub fn get_dominant_file(&mut self) -> SqliteResult<Option<DominantFile>> {
        let stmt = Self::prepare_cached(
            self.db,
            &mut self.stmts.get_dominant_file,
            r#"
        SELECT n.file_path AS file_path, COUNT(*) AS edge_count
        FROM edges e
        JOIN nodes n ON e.source = n.id
        JOIN nodes m ON e.target = m.id
        WHERE n.file_path = m.file_path
        GROUP BY n.file_path
        ORDER BY edge_count DESC
        LIMIT 20
      "#,
        )?;
        let rows = stmt.all(SqliteParams::none())?;
        let filtered = rows
            .into_iter()
            .filter(|row| {
                row_optional_string(row, "file_path")
                    .map(|path| !is_low_value_file(&path))
                    .unwrap_or(false)
            })
            .collect::<Vec<_>>();
        // 少于 20 条内部边通常只是普通小文件；不把它包装成 dominant file，
        // 以免启动提示把 agent 带向无关实现细节。
        if filtered.is_empty() || row_i64(&filtered[0], "edge_count")? < 20 {
            return Ok(None);
        }
        Ok(Some(DominantFile {
            file_path: row_string(&filtered[0], "file_path")?,
            edge_count: row_i64(&filtered[0], "edge_count")?,
            next_edge_count: filtered
                .get(1)
                .and_then(|row| row_i64(row, "edge_count").ok())
                .unwrap_or(0),
        }))
    }

    /// Find the file with the densest concentration of route nodes.
    pub fn get_top_route_file(&mut self) -> SqliteResult<Option<TopRouteFile>> {
        let stmt = Self::prepare_cached(
            self.db,
            &mut self.stmts.get_top_route_file,
            r#"
        SELECT file_path, COUNT(*) AS cnt
        FROM nodes
        WHERE kind = 'route'
        GROUP BY file_path
        ORDER BY cnt DESC
        LIMIT 20
      "#,
        )?;
        let rows = stmt.all(SqliteParams::none())?;
        let filtered = rows
            .into_iter()
            .filter(|row| {
                row_optional_string(row, "file_path")
                    .map(|path| !is_low_value_file(&path))
                    .unwrap_or(false)
            })
            .collect::<Vec<_>>();
        if filtered.is_empty() {
            return Ok(None);
        }

        let total_routes = filtered
            .iter()
            .map(|row| row_i64(row, "cnt").unwrap_or(0))
            .sum::<i64>();
        let top_count = row_i64(&filtered[0], "cnt")?;
        if total_routes < 3 || top_count < 3 {
            return Ok(None);
        }
        // 只有路由明显集中在一个文件时才暴露 top route file；分散式路由
        // 框架下返回 None 更诚实。
        if (top_count as f64) / (total_routes as f64) < 0.30 {
            return Ok(None);
        }
        Ok(Some(TopRouteFile {
            file_path: row_string(&filtered[0], "file_path")?,
            route_count: top_count,
            total_routes,
        }))
    }

    /// Build a URL-to-handler manifest from route edges.
    pub fn get_routing_manifest(
        &mut self,
        limit: Option<i64>,
    ) -> SqliteResult<Option<RoutingManifest>> {
        let stmt = Self::prepare_cached(
            self.db,
            &mut self.stmts.get_routing_manifest,
            r#"
        SELECT
          r.name AS url,
          h.name AS handler,
          h.file_path AS handler_file,
          h.start_line AS handler_line,
          h.kind AS handler_kind
        FROM nodes r
        JOIN edges e ON e.source = r.id
        JOIN nodes h ON e.target = h.id
        WHERE r.kind = 'route'
          AND e.kind IN ('references', 'calls')
          AND h.kind IN ('function', 'method', 'class')
        ORDER BY r.file_path, r.start_line
        LIMIT ?
      "#,
        )?;
        let rows = stmt.all(positional(vec![limit.unwrap_or(40)]))?;
        let filtered = rows
            .into_iter()
            .filter(|row| {
                row_optional_string(row, "handler_file")
                    .map(|path| !is_low_value_file(&path))
                    .unwrap_or(false)
            })
            .collect::<Vec<_>>();
        // manifest 太短时通常不足以代表项目路由结构，直接省略可减少
        // MCP initialize 中低价值噪音。
        if filtered.len() < 3 {
            return Ok(None);
        }

        let mut file_counts: HashMap<String, i64> = HashMap::new();
        for row in &filtered {
            let file = row_string(row, "handler_file")?;
            *file_counts.entry(file).or_insert(0) += 1;
        }
        let (top_handler_file, top_handler_file_count) = file_counts
            .into_iter()
            .max_by_key(|(_, count)| *count)
            .map(|(file, count)| (Some(file), count))
            .unwrap_or((None, 0));

        let mut entries = Vec::new();
        for row in &filtered {
            entries.push(RoutingManifestEntry {
                url: row_string(row, "url")?,
                handler: row_string(row, "handler")?,
                handler_file: row_string(row, "handler_file")?,
                handler_line: row_i64(row, "handler_line")?,
                handler_kind: row_string(row, "handler_kind")?,
            });
        }

        Ok(Some(RoutingManifest {
            entries,
            top_handler_file,
            top_handler_file_count,
            total_routes: filtered.len() as i64,
        }))
    }
}
