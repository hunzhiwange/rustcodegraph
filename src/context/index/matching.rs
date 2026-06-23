//! Extra recall passes for query symbols that are not exact node names.
//!
//! 这些启发式补在精确匹配和全文搜索之后：当用户说 `request`，真实类型可能叫
//! `DataRequest`；当用户给多个词，真实符号可能是这些词的组合。分数要足够高
//! 才能救回目标，但仍会过滤测试文件，避免普通 query 被 fixture 命中。

use std::collections::{HashMap, HashSet};

use crate::db::sqlite_adapter::SqliteResult;
use crate::search::query_utils::{is_test_file, score_path_relevance};
use crate::types::{Node, SearchResult};

use super::ContextBuilder;
use super::options::{ceil_div, definition_node_kinds, search_options};
use super::terms::{sort_results, title_case_identifier};

impl<'a, 'db> ContextBuilder<'a, 'db> {
    pub(super) fn add_camel_and_compound_matches(
        &mut self,
        query: &str,
        symbols_from_query: &[String],
        search_limit: usize,
        is_test_query: bool,
        search_results: &mut Vec<SearchResult>,
    ) -> SqliteResult<()> {
        let camel_definition_kinds = definition_node_kinds();
        let mut camel_searched_terms = HashSet::<String>::new();
        let mut search_id_set = search_results
            .iter()
            .map(|result| result.node.id.clone())
            .collect::<HashSet<_>>();
        let mut camel_node_terms = HashMap::<String, (SearchResult, usize)>::new();
        let max_camel_per_term = ceil_div(search_limit, 2);

        // Camel/Pascal 子串只接受出现在标识符中间的位置，避免 `Task` 同时匹配
        // `Task` 本身和所有前缀结果；这里主要想捕获 `DataTask` 这类复合名。
        for symbol in symbols_from_query {
            let title_cased = title_case_identifier(symbol);
            if title_cased.len() < 3 {
                continue;
            }
            let term_key = title_cased.to_ascii_lowercase();
            if !camel_searched_terms.insert(term_key) {
                continue;
            }

            let like_results = self.queries.find_nodes_by_name_substring(
                &title_cased,
                search_options(200, Some(camel_definition_kinds.clone())),
                true,
            )?;
            let mut term_candidates = Vec::<SearchResult>::new();
            for result in like_results {
                let name = &result.node.name;
                let Some(idx) = name.find(&title_cased) else {
                    continue;
                };
                if idx == 0 {
                    continue;
                }
                let prev = name[..idx].chars().last();
                if !prev.is_some_and(|ch| ch.is_ascii_alphabetic()) {
                    continue;
                }
                if search_id_set.contains(&result.node.id) {
                    continue;
                }
                if is_test_file(&result.node.file_path) && !is_test_query {
                    continue;
                }

                let path_score = score_path_relevance(&result.node.file_path, query, None);
                let brevity_bonus =
                    (6.0 - (name.len() as f64 - title_cased.len() as f64) / 4.0).max(0.0);
                term_candidates.push(SearchResult {
                    node: result.node,
                    score: 8.0 + brevity_bonus + path_score,
                    highlights: None,
                });
            }
            sort_results(&mut term_candidates);

            for result in term_candidates
                .into_iter()
                .take(max_camel_per_term.saturating_mul(4))
            {
                camel_node_terms
                    .entry(result.node.id.clone())
                    .and_modify(|(_, term_count)| *term_count += 1)
                    .or_insert((result, 1));
            }
        }

        let mut camel_results = camel_node_terms
            .into_values()
            .map(|(mut result, term_count)| {
                // 多个 query 词命中同一候选时大幅加权，这是“复合概念”的强信号。
                result.score = result.score * (1.0 + term_count as f64)
                    + term_count.saturating_sub(1) as f64 * 30.0;
                result
            })
            .collect::<Vec<_>>();
        sort_results(&mut camel_results);
        for result in camel_results.into_iter().take(search_limit) {
            search_id_set.insert(result.node.id.clone());
            search_results.push(result);
        }

        if symbols_from_query.len() >= 2 {
            let mut compound_term_map = HashMap::<String, (Node, HashSet<String>)>::new();
            // 第二轮允许子串出现在任意位置，目标是找同时包含多个 query 概念的定义名。
            for symbol in symbols_from_query {
                let title_cased = title_case_identifier(symbol);
                if title_cased.len() < 3 {
                    continue;
                }
                let like_results = self.queries.find_nodes_by_name_substring(
                    &title_cased,
                    search_options(200, Some(camel_definition_kinds.clone())),
                    false,
                )?;
                for result in like_results {
                    if search_id_set.contains(&result.node.id) {
                        continue;
                    }
                    if is_test_file(&result.node.file_path) && !is_test_query {
                        continue;
                    }
                    compound_term_map
                        .entry(result.node.id.clone())
                        .and_modify(|(_, terms)| {
                            terms.insert(title_cased.clone());
                        })
                        .or_insert_with(|| {
                            let mut terms = HashSet::new();
                            terms.insert(title_cased.clone());
                            (result.node, terms)
                        });
                }
            }

            let mut compound_results = Vec::<SearchResult>::new();
            for (_, (node, terms)) in compound_term_map {
                if terms.len() >= 2 {
                    let path_score = score_path_relevance(&node.file_path, query, None);
                    let brevity_bonus = (6.0 - node.name.len() as f64 / 8.0).max(0.0);
                    compound_results.push(SearchResult {
                        node,
                        score: 10.0
                            + terms.len().saturating_sub(1) as f64 * 20.0
                            + path_score
                            + brevity_bonus,
                        highlights: None,
                    });
                }
            }
            sort_results(&mut compound_results);
            for result in compound_results.into_iter().take(ceil_div(search_limit, 2)) {
                search_id_set.insert(result.node.id.clone());
                search_results.push(result);
            }
        }

        Ok(())
    }
}
