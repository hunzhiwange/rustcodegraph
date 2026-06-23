use super::*;

// 搜索路径面向 agent 的“先找符号再扩上下文”工作流：优先 FTS，
// 再逐级降级到 LIKE/fuzzy，并在最后用路径与节点类型重新排序。
impl<'db> QueryBuilder<'db> {
    /// Search nodes by name using FTS with LIKE and fuzzy fallbacks.
    pub fn search_nodes(
        &mut self,
        query: &str,
        options: SearchOptions,
    ) -> SqliteResult<Vec<SearchResult>> {
        let limit = options.limit.unwrap_or(100) as usize;
        let offset = options.offset.unwrap_or(0) as usize;
        let parsed = parse_query(query);

        // 查询串里的 kind:/lang: 过滤和 API options 取并集，调用方可以
        // 提供默认范围，用户文本也能临时收窄范围。
        let kinds = if parsed.kinds.is_empty() {
            options.kinds.clone()
        } else {
            Some(
                options
                    .kinds
                    .clone()
                    .unwrap_or_default()
                    .into_iter()
                    .chain(parsed.kinds.clone())
                    .collect::<HashSet<_>>()
                    .into_iter()
                    .collect(),
            )
        };
        let languages = if parsed.languages.is_empty() {
            options.languages.clone()
        } else {
            Some(
                options
                    .languages
                    .clone()
                    .unwrap_or_default()
                    .into_iter()
                    .chain(parsed.languages.clone())
                    .collect::<HashSet<_>>()
                    .into_iter()
                    .collect(),
            )
        };

        let text = parsed.text.clone();
        let mut results = if text.is_empty() {
            self.search_all_by_filters(kinds.clone(), languages.clone(), limit * 5)?
        } else {
            self.search_nodes_fts(&text, kinds.clone(), languages.clone(), limit, offset)?
        };

        if results.is_empty() && text.len() >= 2 {
            results =
                self.search_nodes_like(&text, kinds.clone(), languages.clone(), limit, offset)?;
        }
        if results.is_empty() && text.len() >= 3 {
            results = self.search_nodes_fuzzy(&text, kinds.clone(), languages.clone(), limit)?;
        }

        if !results.is_empty() && !query.is_empty() {
            // FTS 对短精确 token 有时会因分词/前缀规则漏掉直接命名的符号。
            // 补一轮大小写不敏感 exact name，避免 agent 搜一个函数名却看不到
            // 完全同名定义。
            let mut existing_ids = results
                .iter()
                .map(|result| result.node.id.clone())
                .collect::<HashSet<_>>();
            let max_fts_score = results
                .iter()
                .map(|result| result.score)
                .fold(f64::NEG_INFINITY, f64::max);
            for term in query.split_whitespace().filter(|term| term.len() >= 2) {
                let mut sql = "SELECT * FROM nodes WHERE name = ? COLLATE NOCASE".to_string();
                let mut params = vec![SqliteValue::from(term)];
                append_kind_language_filters(
                    &mut sql,
                    &mut params,
                    "kind",
                    "language",
                    &kinds,
                    &languages,
                )?;
                sql.push_str(" LIMIT 20");
                let mut stmt = self.db.prepare(&sql)?;
                for row in stmt.all(SqliteParams::positional(params))? {
                    let node = row_to_node(&row)?;
                    if existing_ids.insert(node.id.clone()) {
                        results.push(SearchResult {
                            node,
                            score: max_fts_score,
                            highlights: None,
                        });
                    }
                }
            }
        }

        if !results.is_empty() && (!text.is_empty() || !query.is_empty()) {
            // FTS 的 bm25 只知道文本匹配；这里叠加“节点类型、路径相关性、
            // 名称匹配度”，让返回顺序更接近代码导航场景。
            let scoring_query = if text.is_empty() { query } else { &text };
            for result in &mut results {
                result.score += kind_bonus(result.node.kind)
                    + score_path_relevance(
                        &result.node.file_path,
                        scoring_query,
                        Some(&self.project_name_tokens),
                    )
                    + name_match_bonus(&result.node.name, scoring_query);
            }
            results.sort_by(|a, b| b.score.total_cmp(&a.score));
            if results.len() > limit {
                results.truncate(limit);
            }
        }

        if !parsed.path_filters.is_empty() {
            let lowered = parsed
                .path_filters
                .iter()
                .map(|path| path.to_lowercase())
                .collect::<Vec<_>>();
            results.retain(|result| {
                let file_path = result.node.file_path.to_lowercase();
                lowered.iter().any(|path| file_path.contains(path))
            });
        }
        if !parsed.name_filters.is_empty() {
            let lowered = parsed
                .name_filters
                .iter()
                .map(|name| name.to_lowercase())
                .collect::<Vec<_>>();
            results.retain(|result| {
                let name = result.node.name.to_lowercase();
                lowered.iter().any(|filter| name.contains(filter))
            });
        }

        Ok(results)
    }

    fn search_all_by_filters(
        &mut self,
        kinds: Option<Vec<NodeKind>>,
        languages: Option<Vec<Language>>,
        limit: usize,
    ) -> SqliteResult<Vec<SearchResult>> {
        let mut sql = "SELECT * FROM nodes WHERE 1=1".to_string();
        let mut params = Vec::new();
        append_kind_language_filters(
            &mut sql,
            &mut params,
            "kind",
            "language",
            &kinds,
            &languages,
        )?;
        sql.push_str(" ORDER BY name LIMIT ?");
        params.push(SqliteValue::from(limit));
        let mut stmt = self.db.prepare(&sql)?;
        stmt.all(SqliteParams::positional(params))?
            .into_iter()
            .map(|row| {
                Ok(SearchResult {
                    node: row_to_node(&row)?,
                    score: 1.0,
                    highlights: None,
                })
            })
            .collect()
    }

    fn search_nodes_fuzzy(
        &mut self,
        text: &str,
        kinds: Option<Vec<NodeKind>>,
        languages: Option<Vec<Language>>,
        limit: usize,
    ) -> SqliteResult<Vec<SearchResult>> {
        // fuzzy 是最后兜底，候选来自 DISTINCT name；先按编辑距离缩小名字集合，
        // 再回表取节点，避免对 nodes 全表逐行算复杂得分。
        let lowered = text.to_lowercase();
        let max_dist = if lowered.len() <= 4 { 1 } else { 2 };
        let mut candidates = self
            .get_all_node_names()?
            .into_iter()
            .filter_map(|name| {
                let dist = bounded_edit_distance(&name.to_lowercase(), &lowered, max_dist);
                if dist <= max_dist {
                    Some((name, dist))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        candidates.sort_by_key(|(_, dist)| *dist);

        let followup_cap = std::cmp::max(limit * 2, 50);
        let mut results = Vec::new();
        let mut seen = HashSet::new();
        for (name, dist) in candidates.into_iter().take(followup_cap) {
            if results.len() >= limit {
                break;
            }
            let mut sql = "SELECT * FROM nodes WHERE name = ?".to_string();
            let mut params = vec![SqliteValue::from(name)];
            append_kind_language_filters(
                &mut sql,
                &mut params,
                "kind",
                "language",
                &kinds,
                &languages,
            )?;
            sql.push_str(" LIMIT 5");
            let mut stmt = self.db.prepare(&sql)?;
            for row in stmt.all(SqliteParams::positional(params))? {
                let node = row_to_node(&row)?;
                if !seen.insert(node.id.clone()) {
                    continue;
                }
                results.push(SearchResult {
                    node,
                    score: 1.0 / (1.0 + dist as f64),
                    highlights: None,
                });
                if results.len() >= limit {
                    break;
                }
            }
        }
        Ok(results)
    }

    fn search_nodes_fts(
        &mut self,
        query: &str,
        kinds: Option<Vec<NodeKind>>,
        languages: Option<Vec<Language>>,
        limit: usize,
        offset: usize,
    ) -> SqliteResult<Vec<SearchResult>> {
        // FTS5 语法对引号、运算符和冒号很敏感；先把用户输入降成安全的
        // 前缀 OR 查询，失败时返回空结果让 LIKE 路径接手。
        let fts_query = query
            .replace("::", " ")
            .chars()
            .map(|ch| if "'\"*():^".contains(ch) { ' ' } else { ch })
            .collect::<String>()
            .split_whitespace()
            .filter(|term| {
                !matches!(
                    term.to_ascii_uppercase().as_str(),
                    "AND" | "OR" | "NOT" | "NEAR"
                )
            })
            .map(|term| format!("\"{term}\"*"))
            .collect::<Vec<_>>()
            .join(" OR ");
        if fts_query.is_empty() {
            return Ok(Vec::new());
        }

        let fts_limit = std::cmp::max(limit * 5, 100);
        let mut sql = r#"
      SELECT nodes.*, bm25(nodes_fts, 0, 20, 5, 1, 2) as score
      FROM nodes_fts
      JOIN nodes ON nodes_fts.id = nodes.id
      WHERE nodes_fts MATCH ?
    "#
        .to_string();
        let mut params = vec![SqliteValue::from(fts_query)];
        append_kind_language_filters(
            &mut sql,
            &mut params,
            "nodes.kind",
            "nodes.language",
            &kinds,
            &languages,
        )?;
        sql.push_str(" ORDER BY score LIMIT ? OFFSET ?");
        params.push(SqliteValue::from(fts_limit));
        params.push(SqliteValue::from(offset));

        let mut stmt = self.db.prepare(&sql)?;
        match stmt.all(SqliteParams::positional(params)) {
            Ok(rows) => rows
                .into_iter()
                .map(|row| {
                    Ok(SearchResult {
                        node: row_to_node(&row)?,
                        score: row_value(&row, "score")
                            .ok()
                            .and_then(SqliteValue::as_f64)
                            .unwrap_or(0.0)
                            .abs(),
                        highlights: None,
                    })
                })
                .collect(),
            Err(_) => Ok(Vec::new()),
        }
    }

    fn search_nodes_like(
        &mut self,
        query: &str,
        kinds: Option<Vec<NodeKind>>,
        languages: Option<Vec<Language>>,
        limit: usize,
        offset: usize,
    ) -> SqliteResult<Vec<SearchResult>> {
        let mut sql = r#"
      SELECT nodes.*,
        CASE
          WHEN name = ? THEN 1.0
          WHEN name LIKE ? THEN 0.9
          WHEN name LIKE ? THEN 0.8
          WHEN qualified_name LIKE ? THEN 0.7
          ELSE 0.5
        END as score
      FROM nodes
      WHERE (
        name LIKE ? OR
        qualified_name LIKE ? OR
        name LIKE ?
      )
    "#
        .to_string();
        let starts_with = format!("{query}%");
        let contains = format!("%{query}%");
        let mut params = vec![
            SqliteValue::from(query),
            SqliteValue::from(starts_with.clone()),
            SqliteValue::from(contains.clone()),
            SqliteValue::from(contains.clone()),
            SqliteValue::from(contains.clone()),
            SqliteValue::from(contains),
            SqliteValue::from(starts_with),
        ];
        append_kind_language_filters(
            &mut sql,
            &mut params,
            "kind",
            "language",
            &kinds,
            &languages,
        )?;
        sql.push_str(" ORDER BY score DESC, length(name) ASC LIMIT ? OFFSET ?");
        params.push(SqliteValue::from(limit));
        params.push(SqliteValue::from(offset));

        let mut stmt = self.db.prepare(&sql)?;
        stmt.all(SqliteParams::positional(params))?
            .into_iter()
            .map(|row| {
                Ok(SearchResult {
                    node: row_to_node(&row)?,
                    score: row_value(&row, "score")
                        .ok()
                        .and_then(SqliteValue::as_f64)
                        .unwrap_or(0.0),
                    highlights: None,
                })
            })
            .collect()
    }
}
