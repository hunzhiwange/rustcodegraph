//! Option normalization for context building.
//!
//! 外部 API 使用 `Option<Count>` 保持 JS/JSON 调用友好；内部先解析成具体 usize，
//! 让 search/source 子模块不用反复处理缺省值。默认值偏小，保证 context API
//! 是轻量入口，而不是替代 explore/node 的大输出工具。

use crate::types::{
    BuildContextOptions, ContextFormat, Count, EdgeKind, FindRelevantContextOptions, NodeKind,
    SearchOptions, TaskInput,
};

const DEFAULT_MAX_NODES: usize = 20;
const DEFAULT_MAX_CODE_BLOCKS: usize = 5;
const DEFAULT_MAX_CODE_BLOCK_SIZE: usize = 1500;
const DEFAULT_SEARCH_LIMIT: usize = 3;
const DEFAULT_TRAVERSAL_DEPTH: usize = 1;
const DEFAULT_MIN_SCORE: f64 = 0.3;

#[derive(Debug, Clone)]
pub(super) struct ResolvedBuildOptions {
    pub(super) max_nodes: usize,
    pub(super) max_code_blocks: usize,
    pub(super) max_code_block_size: usize,
    pub(super) include_code: bool,
    pub(super) format: ContextFormat,
    pub(super) search_limit: usize,
    pub(super) traversal_depth: usize,
    pub(super) min_score: f64,
}

impl ResolvedBuildOptions {
    pub(super) fn resolve(options: Option<BuildContextOptions>) -> Self {
        let options = options.unwrap_or(BuildContextOptions {
            max_nodes: None,
            max_code_blocks: None,
            max_code_block_size: None,
            include_code: None,
            format: None,
            search_limit: None,
            traversal_depth: None,
            min_score: None,
        });

        Self {
            max_nodes: option_count(options.max_nodes, DEFAULT_MAX_NODES),
            max_code_blocks: option_count(options.max_code_blocks, DEFAULT_MAX_CODE_BLOCKS),
            max_code_block_size: option_count(
                options.max_code_block_size,
                DEFAULT_MAX_CODE_BLOCK_SIZE,
            ),
            include_code: options.include_code.unwrap_or(true),
            format: options.format.unwrap_or(ContextFormat::Markdown),
            search_limit: option_count(options.search_limit, DEFAULT_SEARCH_LIMIT),
            traversal_depth: option_count(options.traversal_depth, DEFAULT_TRAVERSAL_DEPTH),
            min_score: options.min_score.unwrap_or(DEFAULT_MIN_SCORE),
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct ResolvedFindOptions {
    pub(super) search_limit: usize,
    pub(super) traversal_depth: usize,
    pub(super) max_nodes: usize,
    pub(super) min_score: f64,
    pub(super) edge_kinds: Vec<EdgeKind>,
    pub(super) node_kinds: Vec<NodeKind>,
}

impl ResolvedFindOptions {
    pub(super) fn resolve(options: Option<FindRelevantContextOptions>) -> Self {
        let options = options.unwrap_or(FindRelevantContextOptions {
            search_limit: None,
            traversal_depth: None,
            max_nodes: None,
            min_score: None,
            edge_kinds: None,
            node_kinds: None,
        });

        Self {
            search_limit: option_count(options.search_limit, DEFAULT_SEARCH_LIMIT),
            traversal_depth: option_count(options.traversal_depth, DEFAULT_TRAVERSAL_DEPTH),
            max_nodes: option_count(options.max_nodes, DEFAULT_MAX_NODES),
            min_score: options.min_score.unwrap_or(DEFAULT_MIN_SCORE),
            edge_kinds: options.edge_kinds.unwrap_or_default(),
            // find_relevant_context 默认只扩高价值节点，避免 traversal 被参数、字段等
            // 低信号节点占满；全文搜索另有更宽的默认 kind 集合。
            node_kinds: options.node_kinds.unwrap_or_else(high_value_node_kinds),
        }
    }
}

pub(super) fn task_input_query(input: TaskInput) -> String {
    match input {
        TaskInput::Query(query) => query,
        TaskInput::Details(details) => match details.description {
            Some(description) if !description.is_empty() => {
                format!("{}: {}", details.title, description)
            }
            _ => details.title,
        },
    }
}

fn option_count(value: Option<Count>, default: usize) -> usize {
    value.map(|value| value as usize).unwrap_or(default)
}

pub(super) fn high_value_node_kinds() -> Vec<NodeKind> {
    vec![
        NodeKind::Function,
        NodeKind::Method,
        NodeKind::Class,
        NodeKind::Interface,
        NodeKind::TypeAlias,
        NodeKind::Struct,
        NodeKind::Trait,
        NodeKind::Component,
        NodeKind::Route,
        NodeKind::Variable,
        NodeKind::Constant,
        NodeKind::Enum,
        NodeKind::Module,
        NodeKind::Namespace,
    ]
}

pub(super) fn default_text_search_kinds() -> Vec<NodeKind> {
    // 文本检索是 recall 兜底，需要覆盖更多节点类型；后续排名和 cap 会再收窄。
    vec![
        NodeKind::File,
        NodeKind::Module,
        NodeKind::Class,
        NodeKind::Struct,
        NodeKind::Interface,
        NodeKind::Trait,
        NodeKind::Protocol,
        NodeKind::Function,
        NodeKind::Method,
        NodeKind::Property,
        NodeKind::Field,
        NodeKind::Variable,
        NodeKind::Constant,
        NodeKind::Enum,
        NodeKind::EnumMember,
        NodeKind::TypeAlias,
        NodeKind::Namespace,
        NodeKind::Export,
        NodeKind::Route,
        NodeKind::Component,
    ]
}

pub(super) fn definition_node_kinds() -> Vec<NodeKind> {
    // Camel/compound 补召回只看定义类节点，避免变量/属性的普通词子串制造噪声。
    vec![
        NodeKind::Class,
        NodeKind::Interface,
        NodeKind::Struct,
        NodeKind::Trait,
        NodeKind::Protocol,
        NodeKind::Enum,
        NodeKind::TypeAlias,
    ]
}

pub(super) fn non_empty_kinds(kinds: &[NodeKind]) -> Option<Vec<NodeKind>> {
    (!kinds.is_empty()).then(|| kinds.to_vec())
}

pub(super) fn non_empty_edges(kinds: &[EdgeKind]) -> Option<Vec<EdgeKind>> {
    (!kinds.is_empty()).then(|| kinds.to_vec())
}

pub(super) fn search_options(limit: usize, kinds: Option<Vec<NodeKind>>) -> SearchOptions {
    SearchOptions {
        kinds,
        languages: None,
        include_patterns: None,
        exclude_patterns: None,
        limit: Some(limit as Count),
        offset: None,
        case_sensitive: None,
    }
}

pub(super) fn ceil_mul(value: usize, multiplier: usize) -> usize {
    value.saturating_mul(multiplier)
}

pub(super) fn ceil_div(value: usize, divisor: usize) -> usize {
    if divisor == 0 {
        return value;
    }
    value.div_ceil(divisor)
}
