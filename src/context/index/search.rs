//! Query-to-subgraph retrieval pipeline.
//!
//! 这里把一个自然语言/符号混合 query 变成小 subgraph：先精确命中符号，再用文本、
//! camel/compound 和路径相关性补召回，最后做图遍历与预算裁剪。排序启发式直接影响
//! agent 是否还会回退到 Read/Grep，因此宁可保守解释低置信度，也不要给出看似确定的错答案。

use std::collections::{HashMap, HashSet};

use crate::db::sqlite_adapter::SqliteResult;
use crate::graph::traversal::GraphTraverser;
use crate::search::query_utils::{
    extract_search_terms, get_stem_variants, is_distinctive_identifier, is_test_file,
};
use crate::types::{
    Confidence, Count, Edge, EdgeKind, FindRelevantContextOptions, Node, NodeKind, SearchResult,
    Subgraph, TraversalDirection, TraversalOptions,
};

use super::ContextBuilder;
use super::graph_utils::{
    apply_file_diversity_cap, apply_non_production_cap, insert_node, is_type_hierarchy_kind,
    make_subgraph, node_sort_key, push_edge_unique, sorted_node_ids, trim_to_max_nodes,
};
use super::options::{
    ResolvedFindOptions, ceil_div, ceil_mul, default_text_search_kinds, definition_node_kinds,
    non_empty_edges, non_empty_kinds, search_options,
};
use super::terms::{
    dominant_file_dir, extract_symbols_from_query, grouped_substring_terms, path_dirname,
    sort_results, title_case_identifier, upsert_result,
};

impl<'a, 'db> ContextBuilder<'a, 'db> {
    /// Find relevant subgraph for a query.
    pub fn find_relevant_context(
        &mut self,
        query: &str,
        options: Option<FindRelevantContextOptions>,
    ) -> SqliteResult<Subgraph> {
        let opts = ResolvedFindOptions::resolve(options);
        let mut nodes = HashMap::<String, Node>::new();
        let mut edges = Vec::<Edge>::new();
        let mut roots = Vec::<String>::new();

        if query.trim().is_empty() {
            return Ok(make_subgraph(nodes, edges, roots, None));
        }

        let symbols_from_query = extract_symbols_from_query(query);

        let mut exact_matches = Vec::<SearchResult>::new();
        if !symbols_from_query.is_empty() {
            // 精确 symbol 是最高信号；limit 先放大，后面还有同文件 co-mention 和分数裁剪。
            exact_matches = self.queries.find_nodes_by_exact_name(
                &symbols_from_query,
                search_options(
                    ceil_mul(opts.search_limit, 5),
                    non_empty_kinds(&opts.node_kinds),
                ),
            )?;

            if exact_matches.len() > 1 {
                let mut file_symbol_counts = HashMap::<String, HashSet<String>>::new();
                // 一个文件同时命中多个 query symbol，比零散同名符号更可能是目标流程。
                for result in &exact_matches {
                    file_symbol_counts
                        .entry(result.node.file_path.clone())
                        .or_default()
                        .insert(result.node.name.to_ascii_lowercase());
                }

                for result in &mut exact_matches {
                    let symbol_count = file_symbol_counts
                        .get(&result.node.file_path)
                        .map(HashSet::len)
                        .unwrap_or(1);
                    if symbol_count > 1 {
                        result.score += (symbol_count - 1) as f64 * 20.0;
                    }
                }
                sort_results(&mut exact_matches);
            }

            exact_matches.truncate(ceil_mul(opts.search_limit, 2));
        }

        if !symbols_from_query.is_empty() {
            let definition_kinds = definition_node_kinds();
            let mut expanded_symbols = symbols_from_query.iter().cloned().collect::<HashSet<_>>();
            for symbol in &symbols_from_query {
                for variant in get_stem_variants(symbol) {
                    expanded_symbols.insert(variant);
                }
            }
            let mut expanded = expanded_symbols.into_iter().collect::<Vec<_>>();
            expanded.sort();

            // 用户常输入 lower/camel 片段，而定义可能是 PascalCase 前缀；
            // 这轮只扩定义类节点，避免普通变量前缀泛滥。
            for symbol in expanded {
                let title_cased = title_case_identifier(&symbol);
                if title_cased == symbol {
                    continue;
                }

                let prefix_results = self.queries.search_nodes(
                    &title_cased,
                    search_options(30, Some(definition_kinds.clone())),
                )?;
                let mut matched = Vec::new();
                for mut result in prefix_results {
                    if result
                        .node
                        .name
                        .to_ascii_lowercase()
                        .starts_with(&title_cased.to_ascii_lowercase())
                    {
                        let brevity_bonus = (10.0
                            - (result.node.name.len() as f64 - title_cased.len() as f64) / 3.0)
                            .max(0.0);
                        result.score += 15.0 + brevity_bonus;
                        matched.push(result);
                    }
                }
                sort_results(&mut matched);
                for result in matched.into_iter().take(opts.search_limit) {
                    if !exact_matches
                        .iter()
                        .any(|existing| existing.node.id == result.node.id)
                    {
                        exact_matches.push(result);
                    }
                }
            }
            sort_results(&mut exact_matches);
            exact_matches.truncate(ceil_mul(opts.search_limit, 3));
        }

        let mut text_results = Vec::<SearchResult>::new();
        let search_terms = extract_search_terms(query, Some(true));
        if !search_terms.is_empty() {
            // 文本搜索是兜底 recall。多词命中会累加小额奖励，但不能盖过强 symbol 信号。
            let search_kinds = if opts.node_kinds.is_empty() {
                default_text_search_kinds()
            } else {
                opts.node_kinds.clone()
            };
            let mut term_results = HashMap::<String, (SearchResult, usize)>::new();
            for term in search_terms {
                let results = self.queries.search_nodes(
                    &term,
                    search_options(ceil_mul(opts.search_limit, 2), Some(search_kinds.clone())),
                )?;
                for result in results {
                    term_results
                        .entry(result.node.id.clone())
                        .and_modify(|(existing, term_hits)| {
                            *term_hits += 1;
                            existing.score = existing.score.max(result.score);
                        })
                        .or_insert((result, 1));
                }
            }
            text_results = term_results
                .into_values()
                .map(|(mut result, term_hits)| {
                    result.score += (term_hits.saturating_sub(1)) as f64 * 5.0;
                    result
                })
                .collect();
            sort_results(&mut text_results);
            text_results.truncate(ceil_mul(opts.search_limit, 2));
        }

        let mut search_results = Vec::<SearchResult>::new();
        let mut result_index = HashMap::<String, usize>::new();
        for result in exact_matches.iter().cloned() {
            upsert_result(&mut search_results, &mut result_index, result);
        }
        for result in text_results {
            upsert_result(&mut search_results, &mut result_index, result);
        }

        let query_lower = query.to_ascii_lowercase();
        let is_test_query = query_lower.contains("test") || query_lower.contains("spec");
        if !is_test_query {
            // 测试文件命名通常更接近 prose query；非测试问题先降权，稍后还会做数量 cap。
            for result in &mut search_results {
                if is_test_file(&result.node.file_path) {
                    result.score *= 0.3;
                }
            }
        }

        if let Ok(Some(dominant)) = self.queries.get_dominant_file()
            && dominant.edge_count >= 3 * dominant.next_edge_count
            && let Some(core_dir) = dominant_file_dir(&dominant.file_path)
        {
            // 某些小库有一个“核心文件”承载主要边；给同目录加权能提高入口命中率，
            // 但只在它明显支配图结构时启用，避免大仓库被单个热文件带偏。
            for result in &mut search_results {
                if result.node.file_path.starts_with(&core_dir) {
                    result.score += 25.0;
                }
            }
        }

        let query_terms_for_boost = extract_search_terms(query, Some(true));
        if query_terms_for_boost.len() >= 2 {
            let term_groups = grouped_substring_terms(query_terms_for_boost);
            let exact_match_ids = exact_matches
                .iter()
                .map(|result| result.node.id.clone())
                .collect::<HashSet<_>>();
            let distinctive_tokens = symbols_from_query
                .iter()
                .filter(|symbol| is_distinctive_identifier(symbol))
                .map(|symbol| symbol.to_ascii_lowercase())
                .collect::<HashSet<_>>();
            let distinctive_exact_match_ids = exact_matches
                .iter()
                .filter(|result| {
                    distinctive_tokens.contains(&result.node.name.to_ascii_lowercase())
                })
                .map(|result| result.node.id.clone())
                .collect::<HashSet<_>>();

            // 多词 query 需要“共同指向同一候选”的证据。只命中一个普通词的精确匹配
            // 常是陷阱，例如常量名 `FLAT` 抢走 “capture flat object screen”。
            for result in &mut search_results {
                let name_lower = result.node.name.to_ascii_lowercase();
                let dir_segments = path_dirname(&result.node.file_path)
                    .to_ascii_lowercase()
                    .split('/')
                    .map(|segment| segment.to_string())
                    .collect::<Vec<_>>();
                let mut match_count = 0;
                for group in &term_groups {
                    if group.iter().any(|term| {
                        name_lower.contains(term)
                            || dir_segments.iter().any(|segment| segment == term)
                    }) {
                        match_count += 1;
                    }
                }

                if match_count >= 2 {
                    result.score *= 1.0 + match_count as f64 * 0.5;
                } else if distinctive_exact_match_ids.contains(&result.node.id) {
                } else if exact_match_ids.contains(&result.node.id) {
                    result.score *= 0.3;
                } else {
                    result.score *= 0.6;
                }
            }
            sort_results(&mut search_results);
        }

        if !symbols_from_query.is_empty() {
            self.add_camel_and_compound_matches(
                query,
                &symbols_from_query,
                opts.search_limit,
                is_test_query,
                &mut search_results,
            )?;
        }

        sort_results(&mut search_results);
        search_results.truncate(ceil_mul(opts.search_limit, 3));

        let mut filtered_results = search_results
            .into_iter()
            .filter(|result| result.score >= opts.min_score)
            .collect::<Vec<_>>();
        filtered_results = self.resolve_imports_to_definitions(filtered_results)?;
        if filtered_results.len() > opts.search_limit {
            filtered_results.truncate(opts.search_limit);
        }

        let confidence = self.compute_confidence(query, &symbols_from_query, &filtered_results);

        for result in &filtered_results {
            insert_node(&mut nodes, result.node.clone());
            roots.push(result.node.id.clone());
        }

        let max_hierarchy_nodes = ceil_div(opts.max_nodes, 4);
        let mut hierarchy_nodes_added = 0;
        // 类型层级对理解接口/继承很关键，但不能吞掉 context；最多使用总预算的四分之一。
        for result in &filtered_results {
            if hierarchy_nodes_added >= max_hierarchy_nodes {
                break;
            }
            if !is_type_hierarchy_kind(result.node.kind) {
                continue;
            }
            let hierarchy = {
                let mut traverser = GraphTraverser::new(&mut *self.queries);
                traverser.get_type_hierarchy(&result.node.id)?
            };
            for (id, node) in hierarchy.nodes {
                if !nodes.contains_key(&id) {
                    insert_node(&mut nodes, node);
                    hierarchy_nodes_added += 1;
                }
            }
            for edge in hierarchy.edges {
                push_edge_unique(&mut edges, edge);
            }
        }

        if hierarchy_nodes_added > 0 {
            // 第一层层级可能只带来父类/接口；对这些 sibling 再补一次层级，
            // 可以在同一预算内保住 override/implements 的可解释路径。
            let root_set = roots.iter().cloned().collect::<HashSet<_>>();
            let mut pass2_candidates = nodes
                .values()
                .filter(|node| is_type_hierarchy_kind(node.kind) && !root_set.contains(&node.id))
                .cloned()
                .collect::<Vec<_>>();
            pass2_candidates.sort_by_key(node_sort_key);

            for candidate in pass2_candidates {
                if hierarchy_nodes_added >= max_hierarchy_nodes {
                    break;
                }
                let sibling_hierarchy = {
                    let mut traverser = GraphTraverser::new(&mut *self.queries);
                    traverser.get_type_hierarchy(&candidate.id)?
                };
                for (id, node) in sibling_hierarchy.nodes {
                    if !nodes.contains_key(&id) && hierarchy_nodes_added < max_hierarchy_nodes {
                        insert_node(&mut nodes, node);
                        hierarchy_nodes_added += 1;
                    }
                }
                for edge in sibling_hierarchy.edges {
                    if nodes.contains_key(&edge.source) && nodes.contains_key(&edge.target) {
                        push_edge_unique(&mut edges, edge);
                    }
                }
            }
        }

        let per_entry_limit = ceil_div(opts.max_nodes, filtered_results.len().max(1));
        // 每个入口平均分配遍历预算，避免第一个命中点把 BFS limit 用完。
        for result in &filtered_results {
            let traversal_result = {
                let mut traverser = GraphTraverser::new(&mut *self.queries);
                traverser.traverse_bfs(
                    &result.node.id,
                    Some(TraversalOptions {
                        max_depth: Some(opts.traversal_depth as Count),
                        edge_kinds: non_empty_edges(&opts.edge_kinds),
                        node_kinds: non_empty_kinds(&opts.node_kinds),
                        direction: Some(TraversalDirection::Both),
                        limit: Some(per_entry_limit as Count),
                        include_start: None,
                    }),
                )?
            };

            for (_, node) in traversal_result.nodes {
                insert_node(&mut nodes, node);
            }
            for edge in traversal_result.edges {
                push_edge_unique(&mut edges, edge);
            }
        }

        let (mut final_nodes, mut final_edges) =
            trim_to_max_nodes(nodes, edges, &roots, opts.max_nodes);
        apply_file_diversity_cap(&mut final_nodes, &mut roots, opts.max_nodes);
        apply_non_production_cap(&mut final_nodes, &mut roots, opts.max_nodes, is_test_query);
        final_edges.retain(|edge| {
            final_nodes.contains_key(&edge.source) && final_nodes.contains_key(&edge.target)
        });

        let final_node_ids = sorted_node_ids(&final_nodes);
        // 节点裁剪后重新查一次节点间边，能恢复 traversal 未覆盖但最终节点之间真实存在的关系。
        let recovered_edges = self.queries.find_edges_between_nodes(
            &final_node_ids,
            Some(vec![
                EdgeKind::Calls,
                EdgeKind::Extends,
                EdgeKind::Implements,
                EdgeKind::References,
                EdgeKind::Overrides,
            ]),
        )?;
        for edge in recovered_edges {
            push_edge_unique(&mut final_edges, edge);
        }

        Ok(make_subgraph(
            final_nodes,
            final_edges,
            roots,
            Some(confidence),
        ))
    }

    fn resolve_imports_to_definitions(
        &mut self,
        results: Vec<SearchResult>,
    ) -> SqliteResult<Vec<SearchResult>> {
        let mut resolved = Vec::new();
        let mut seen_ids = HashSet::new();

        for result in results {
            let node = &result.node;
            if !matches!(node.kind, NodeKind::Import | NodeKind::Export) {
                if seen_ids.insert(node.id.clone()) {
                    resolved.push(result);
                }
                continue;
            }

            // import/export 节点本身很少是 agent 想看的答案；沿边替换成定义节点，
            // 分数沿用原结果，表示“这个定义是通过导入名被找到的”。
            let edge_kind = if node.kind == NodeKind::Import {
                EdgeKind::Imports
            } else {
                EdgeKind::Exports
            };
            let outgoing_edges =
                self.queries
                    .get_outgoing_edges(&node.id, Some(vec![edge_kind]), None)?;
            for edge in outgoing_edges {
                if let Some(target_node) = self.queries.get_node_by_id(&edge.target)?
                    && seen_ids.insert(target_node.id.clone())
                {
                    resolved.push(SearchResult {
                        node: target_node,
                        score: result.score,
                        highlights: None,
                    });
                }
            }
        }

        Ok(resolved)
    }

    fn compute_confidence(
        &self,
        query: &str,
        symbols_from_query: &[String],
        filtered_results: &[SearchResult],
    ) -> Confidence {
        let conf_terms = extract_search_terms(query, Some(false))
            .into_iter()
            .filter(|term| term.len() >= 3)
            .collect::<Vec<_>>();
        if conf_terms.len() < 2 || filtered_results.is_empty() {
            // 单词或空结果没有足够证据判低置信度；避免把短精确查询误导到补救流程。
            return Confidence::High;
        }

        let distinctive = symbols_from_query
            .iter()
            .filter(|symbol| is_distinctive_identifier(symbol))
            .map(|symbol| symbol.to_ascii_lowercase())
            .collect::<HashSet<_>>();

        let any_strong = filtered_results.iter().any(|result| {
            if distinctive.contains(&result.node.name.to_ascii_lowercase()) {
                return true;
            }
            // 强结果必须在名称或目录中覆盖至少两个 query 概念；否则多半只是普通词命中。
            let name_lower = result.node.name.to_ascii_lowercase();
            let dir_segments = path_dirname(&result.node.file_path)
                .to_ascii_lowercase()
                .split('/')
                .map(|segment| segment.to_string())
                .collect::<Vec<_>>();
            let mut hits = 0;
            for term in &conf_terms {
                if name_lower.contains(term) || dir_segments.iter().any(|segment| segment == term) {
                    hits += 1;
                    if hits >= 2 {
                        return true;
                    }
                }
            }
            false
        });

        if any_strong {
            Confidence::High
        } else {
            Confidence::Low
        }
    }
}
