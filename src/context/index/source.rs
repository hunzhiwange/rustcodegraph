//! Source extraction for context output.
//!
//! DB 节点只保存相对路径和行号；这里负责在项目根下安全读取源码，并按 roots、
//! 函数/方法、类的顺序挑选代码块。配置叶子节点没有稳定源码范围，优先返回签名类摘要。

use std::collections::HashSet;
use std::fs;

use crate::db::sqlite_adapter::SqliteResult;
use crate::types::{CodeBlock, Node, NodeKind, Subgraph};
use crate::utils::{is_config_leaf_node, validate_path_within_root};

use super::super::formatter::{language_name, node_kind_name};
use super::ContextBuilder;
use super::graph_utils::node_sort_key;
use super::terms::truncate_to_char_boundary;

impl<'a, 'db> ContextBuilder<'a, 'db> {
    /// Get the source code for a node.
    pub fn get_code(&mut self, node_id: &str) -> SqliteResult<Option<String>> {
        let Some(node) = self.queries.get_node_by_id(node_id)? else {
            return Ok(None);
        };
        Ok(self.extract_node_code(&node))
    }

    fn extract_node_code(&self, node: &Node) -> Option<String> {
        if is_config_leaf_node(
            &node_kind_name(node.kind),
            Some(language_name(node.language).as_str()),
        ) {
            // JSON/YAML 等配置叶子节点常没有可读的多行范围；签名/qualified name
            // 比按行切文件更稳定，也能避免把整份配置塞进 context。
            if let Some(signature) = &node.signature
                && !signature.is_empty()
            {
                return Some(signature.clone());
            }
            if !node.qualified_name.is_empty() {
                return Some(node.qualified_name.clone());
            }
            return Some(node.name.clone());
        }

        let file_path = validate_path_within_root(&self.project_root, &node.file_path)?;
        // validate_path_within_root 同时处理路径穿越和绝对路径注入；失败时返回 None，
        // 让 context 构建降级为“无代码块”而不是中断整次检索。
        if !file_path.exists() {
            return None;
        }

        let content = fs::read_to_string(file_path).ok()?;
        let lines = content.split('\n').collect::<Vec<_>>();
        let start_idx = node.start_line.saturating_sub(1) as usize;
        let end_idx = (node.end_line as usize).min(lines.len());
        if start_idx >= end_idx || start_idx >= lines.len() {
            return None;
        }

        Some(lines[start_idx..end_idx].join("\n"))
    }

    pub fn get_entry_points(&self, subgraph: &Subgraph) -> Vec<Node> {
        subgraph
            .roots
            .iter()
            .filter_map(|id| subgraph.nodes.get(id).cloned())
            .collect()
    }

    pub(super) fn extract_code_blocks(
        &self,
        subgraph: &Subgraph,
        max_blocks: usize,
        max_block_size: usize,
    ) -> SqliteResult<Vec<CodeBlock>> {
        let mut blocks = Vec::new();
        let root_set = subgraph.roots.iter().cloned().collect::<HashSet<_>>();
        let mut priority_nodes = Vec::<Node>::new();

        // 先给 entry points 源码，随后补函数/方法和类。这样有限 code block 预算
        // 优先服务用户 query 命中的符号，而不是被图遍历的邻居抢走。
        for id in &subgraph.roots {
            if let Some(node) = subgraph.nodes.get(id) {
                priority_nodes.push(node.clone());
            }
        }

        let mut function_nodes = subgraph
            .nodes
            .values()
            .filter(|node| !root_set.contains(&node.id))
            .filter(|node| matches!(node.kind, NodeKind::Function | NodeKind::Method))
            .cloned()
            .collect::<Vec<_>>();
        function_nodes.sort_by_key(node_sort_key);
        priority_nodes.extend(function_nodes);

        let mut class_nodes = subgraph
            .nodes
            .values()
            .filter(|node| !root_set.contains(&node.id))
            .filter(|node| node.kind == NodeKind::Class)
            .cloned()
            .collect::<Vec<_>>();
        class_nodes.sort_by_key(node_sort_key);
        priority_nodes.extend(class_nodes);

        for node in priority_nodes {
            if blocks.len() >= max_blocks {
                break;
            }
            let Some(code) = self.extract_node_code(&node) else {
                continue;
            };
            let content = if code.len() > max_block_size {
                format!(
                    "{}\n... (truncated) ...",
                    truncate_to_char_boundary(&code, max_block_size)
                )
            } else {
                code
            };
            blocks.push(CodeBlock {
                content,
                file_path: node.file_path.clone(),
                start_line: node.start_line,
                end_line: node.end_line,
                language: node.language,
                node: Some(node),
            });
        }

        Ok(blocks)
    }

    pub(super) fn get_related_files(&self, subgraph: &Subgraph) -> Vec<String> {
        let mut files = subgraph
            .nodes
            .values()
            .map(|node| node.file_path.clone())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        files.sort();
        files
    }

    pub(super) fn generate_summary(
        &self,
        _query: &str,
        subgraph: &Subgraph,
        entry_points: &[Node],
    ) -> String {
        let node_count = subgraph.nodes.len();
        let edge_count = subgraph.edges.len();
        let files = self.get_related_files(subgraph);
        let entry_point_names = entry_points
            .iter()
            .take(3)
            .map(|node| node.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let remaining = if entry_points.len() > 3 {
            format!(" and {} more", entry_points.len() - 3)
        } else {
            String::new()
        };

        format!(
            "Found {node_count} relevant code symbols across {} files. Key entry points: {entry_point_names}{remaining}. {edge_count} relationships identified.",
            files.len()
        )
    }
}
