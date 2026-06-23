//! Context builder.
//!
//! Builds task-oriented context by combining symbol search with graph traversal,
//! then formats the result for agent consumption.
//!
//! 这个模块是旧 context API 的门面：先把用户任务归一成检索 query，再委托
//! search/graph/source 子模块拼出 `TaskContext`，最后按调用方需要输出 Markdown
//! 或 JSON。它不直接做解析或索引，所有持久化访问都通过 `QueryBuilder` 完成。

mod call_paths;
mod graph_utils;
mod matching;
mod options;
mod search;
mod source;
mod terms;

use std::path::PathBuf;

use crate::db::queries::QueryBuilder;
use crate::db::sqlite_adapter::SqliteResult;
use crate::types::{
    BuildContextOptions, Confidence, ContextFormat, Count, FindRelevantContextOptions, TaskContext,
    TaskContextStats, TaskInput,
};

use options::{ResolvedBuildOptions, task_input_query};

pub use super::formatter::{
    format_bytes, format_context_as_json, format_context_as_markdown, format_subgraph_tree,
};
pub use super::markers::LOW_CONFIDENCE_MARKER;

/// Return value for `ContextBuilder::build_context`.
#[derive(Debug, Clone)]
pub enum BuildContextResult {
    Context(Box<TaskContext>),
    Formatted(String),
}

/// Coordinates search, traversal, code extraction, and formatting.
pub struct ContextBuilder<'a, 'db> {
    // 源码截取必须以项目根为安全边界；DB 里存的是相对路径，读取前会再次校验。
    project_root: PathBuf,
    queries: &'a mut QueryBuilder<'db>,
}

impl<'a, 'db> ContextBuilder<'a, 'db> {
    pub fn new(project_root: impl Into<PathBuf>, queries: &'a mut QueryBuilder<'db>) -> Self {
        Self {
            project_root: project_root.into(),
            queries,
        }
    }

    /// Build context for a task and return formatted output by default.
    pub fn build_context(
        &mut self,
        input: TaskInput,
        options: Option<BuildContextOptions>,
    ) -> SqliteResult<BuildContextResult> {
        let opts = ResolvedBuildOptions::resolve(options);
        let query = task_input_query(input);

        let subgraph = self.find_relevant_context(
            &query,
            Some(FindRelevantContextOptions {
                search_limit: Some(opts.search_limit as Count),
                traversal_depth: Some(opts.traversal_depth as Count),
                max_nodes: Some(opts.max_nodes as Count),
                min_score: Some(opts.min_score),
                edge_kinds: None,
                node_kinds: None,
            }),
        )?;
        let entry_points = self.get_entry_points(&subgraph);
        let code_blocks = if opts.include_code {
            self.extract_code_blocks(&subgraph, opts.max_code_blocks, opts.max_code_block_size)?
        } else {
            Vec::new()
        };
        let related_files = self.get_related_files(&subgraph);
        let summary = self.generate_summary(&query, &subgraph, &entry_points);
        let stats = TaskContextStats {
            node_count: subgraph.nodes.len() as Count,
            edge_count: subgraph.edges.len() as Count,
            file_count: related_files.len() as Count,
            code_block_count: code_blocks.len() as Count,
            total_code_size: code_blocks
                .iter()
                .map(|block| block.content.len() as Count)
                .sum(),
        };

        let context = TaskContext {
            query,
            subgraph,
            entry_points,
            code_blocks,
            related_files,
            summary,
            stats,
        };

        match opts.format {
            ContextFormat::Markdown => {
                let mut out = format_context_as_markdown(&context);
                // Markdown 是给 agent 直接阅读的通道，所以在结构化 context 之外追加
                // flow hint 和低置信度提示；JSON 保持纯结构化，避免破坏机器消费者。
                out.push_str(&self.build_call_paths_section(&context.subgraph));
                if context.subgraph.confidence == Some(Confidence::Low) {
                    out.push_str(&self.build_low_confidence_note(&context.entry_points));
                }
                Ok(BuildContextResult::Formatted(out))
            }
            ContextFormat::Json => Ok(BuildContextResult::Formatted(format_context_as_json(
                &context,
            ))),
        }
    }

    /// Build and return the structured context without formatting.
    pub fn build_context_struct(
        &mut self,
        input: TaskInput,
        options: Option<BuildContextOptions>,
    ) -> SqliteResult<TaskContext> {
        let opts = ResolvedBuildOptions::resolve(options);
        let query = task_input_query(input);
        let subgraph = self.find_relevant_context(
            &query,
            Some(FindRelevantContextOptions {
                search_limit: Some(opts.search_limit as Count),
                traversal_depth: Some(opts.traversal_depth as Count),
                max_nodes: Some(opts.max_nodes as Count),
                min_score: Some(opts.min_score),
                edge_kinds: None,
                node_kinds: None,
            }),
        )?;
        let entry_points = self.get_entry_points(&subgraph);
        let code_blocks = if opts.include_code {
            self.extract_code_blocks(&subgraph, opts.max_code_blocks, opts.max_code_block_size)?
        } else {
            Vec::new()
        };
        let related_files = self.get_related_files(&subgraph);
        let summary = self.generate_summary(&query, &subgraph, &entry_points);
        let stats = TaskContextStats {
            node_count: subgraph.nodes.len() as Count,
            edge_count: subgraph.edges.len() as Count,
            file_count: related_files.len() as Count,
            code_block_count: code_blocks.len() as Count,
            total_code_size: code_blocks
                .iter()
                .map(|block| block.content.len() as Count)
                .sum(),
        };

        Ok(TaskContext {
            query,
            subgraph,
            entry_points,
            code_blocks,
            related_files,
            summary,
            stats,
        })
    }
}

/// Create a context builder.
pub fn create_context_builder<'a, 'db>(
    project_root: impl Into<PathBuf>,
    queries: &'a mut QueryBuilder<'db>,
) -> ContextBuilder<'a, 'db> {
    ContextBuilder::new(project_root, queries)
}
