use super::*;

// 精确名称查询是 resolver/explore 的高信号路径：它不做全文搜索，
// 只在同名过多时用文件分布和过滤条件帮助排序、限流。
impl<'db> QueryBuilder<'db> {
    /// Find nodes by exact name match.
    pub fn find_nodes_by_exact_name(
        &mut self,
        names: &[String],
        options: SearchOptions,
    ) -> SqliteResult<Vec<SearchResult>> {
        if names.is_empty() {
            return Ok(Vec::new());
        }

        let kinds = options.kinds.clone();
        let languages = options.languages.clone();
        let limit = options.limit.unwrap_or(50) as usize;

        let mut distinctive_files = HashSet::new();
        for name in names {
            // 如果某个名字只出现在少数文件，这些文件很可能就是用户查询的
            // 真实上下文；后续结果给它们加权，降低同名工具函数的干扰。
            let mut sql =
                "SELECT DISTINCT file_path FROM nodes WHERE name COLLATE NOCASE = ?".to_string();
            let mut params = vec![SqliteValue::from(name.clone())];
            if let Some(kinds) = &kinds {
                sql.push_str(&format!(" AND kind IN ({})", placeholders(kinds.len())));
                params.extend(
                    kinds
                        .iter()
                        .map(|kind| db_enum_string(kind, "kind").map(SqliteValue::from))
                        .collect::<SqliteResult<Vec<_>>>()?,
                );
            }
            sql.push_str(" LIMIT 100");
            let mut stmt = self.db.prepare(&sql)?;
            let files = stmt
                .all(SqliteParams::positional(params))?
                .into_iter()
                .filter_map(|row| row_string(&row, "file_path").ok())
                .collect::<HashSet<_>>();
            if !files.is_empty() && files.len() < 10 {
                distinctive_files.extend(files);
            }
        }

        let per_name_limit = std::cmp::max(8, limit.div_ceil(names.len()));
        let mut all_results = Vec::new();
        let mut seen_ids = HashSet::new();

        for name in names {
            // 每个名字先取超额候选，再在内存里按 distinctive file 加权截断；
            // 这样多 token 查询不会被第一个常见名字吃完总 limit。
            let mut sql = r#"
        SELECT nodes.*, 1.0 as score
        FROM nodes
        WHERE name COLLATE NOCASE = ?
      "#
            .to_string();
            let mut params = vec![SqliteValue::from(name.clone())];
            append_kind_language_filters(
                &mut sql,
                &mut params,
                "kind",
                "language",
                &kinds,
                &languages,
            )?;
            sql.push_str(" LIMIT ?");
            params.push(SqliteValue::from(std::cmp::max(per_name_limit * 3, 50)));

            let mut stmt = self.db.prepare(&sql)?;
            let mut name_results = Vec::new();
            for row in stmt.all(SqliteParams::positional(params))? {
                let node = row_to_node(&row)?;
                if seen_ids.contains(&node.id) {
                    continue;
                }
                let boost = if distinctive_files.contains(&node.file_path) {
                    20.0
                } else {
                    0.0
                };
                let score = row_value(&row, "score")
                    .ok()
                    .and_then(SqliteValue::as_f64)
                    .unwrap_or(1.0)
                    + boost;
                name_results.push(SearchResult {
                    node,
                    score,
                    highlights: None,
                });
            }
            name_results.sort_by(|a, b| b.score.total_cmp(&a.score));
            for result in name_results.into_iter().take(per_name_limit) {
                seen_ids.insert(result.node.id.clone());
                all_results.push(result);
            }
        }

        all_results.sort_by(|a, b| b.score.total_cmp(&a.score));
        all_results.truncate(limit);
        Ok(all_results)
    }

    /// Find nodes whose name contains a substring.
    pub fn find_nodes_by_name_substring(
        &mut self,
        substring: &str,
        options: SearchOptions,
        exclude_prefix: bool,
    ) -> SqliteResult<Vec<SearchResult>> {
        let kinds = options.kinds.clone();
        let languages = options.languages.clone();
        let limit = options.limit.unwrap_or(30) as usize;

        let mut sql = r#"
      SELECT nodes.*, 1.0 as score
      FROM nodes
      WHERE name LIKE ?
    "#
        .to_string();
        let mut params = vec![SqliteValue::from(format!("%{substring}%"))];
        if exclude_prefix {
            sql.push_str(" AND name NOT LIKE ?");
            params.push(SqliteValue::from(format!("{substring}%")));
        }
        append_kind_language_filters(
            &mut sql,
            &mut params,
            "kind",
            "language",
            &kinds,
            &languages,
        )?;
        sql.push_str(" ORDER BY length(name) ASC LIMIT ?");
        params.push(SqliteValue::from(limit));

        let mut stmt = self.db.prepare(&sql)?;
        stmt.all(SqliteParams::positional(params))?
            .into_iter()
            .map(|row| {
                Ok(SearchResult {
                    node: row_to_node(&row)?,
                    score: row_value(&row, "score")
                        .ok()
                        .and_then(SqliteValue::as_f64)
                        .unwrap_or(1.0),
                    highlights: None,
                })
            })
            .collect()
    }
}
