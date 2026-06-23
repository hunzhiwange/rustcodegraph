//! CodeGraph type definitions translated from `types.ts`.
//!
//! Numeric choices intentionally mirror the TypeScript serialization shape:
//! timestamps use signed `i64` milliseconds for SQLite-friendly round trips,
//! while line, column, duration, byte-size, and count fields use unsigned
//! `u64` because the TypeScript values are non-negative.
//!
//! 这些类型是 extraction、DB、graph、context 和 MCP 之间的公共契约。
//! 字符串序列化形状要和既有 TypeScript/SQLite 数据保持兼容，新增枚举值时
//! 需要同步考虑查询、迁移、MCP 输出和测试夹具。

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub type TimestampMs = i64;
pub type DurationMs = u64;
pub type LineNumber = u64;
pub type ColumnNumber = u64;
pub type ByteSize = u64;
pub type Count = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    File,
    Module,
    Class,
    Struct,
    Interface,
    Trait,
    Protocol,
    Function,
    Method,
    Property,
    Field,
    Variable,
    Constant,
    Enum,
    EnumMember,
    TypeAlias,
    Namespace,
    Parameter,
    Import,
    Export,
    Route,
    Component,
}

/// 稳定顺序用于校验、CLI 展示和测试快照；不要把它当作按重要性排序。
pub const NODE_KINDS: &[NodeKind] = &[
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
    NodeKind::Parameter,
    NodeKind::Import,
    NodeKind::Export,
    NodeKind::Route,
    NodeKind::Component,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeKind {
    Contains,
    Calls,
    Imports,
    Exports,
    Extends,
    Implements,
    References,
    TypeOf,
    Returns,
    Instantiates,
    Overrides,
    Decorates,
}

/// EdgeKind 是图遍历和 resolver 的共同词表，序列化名称也是数据库里的边类型。
pub const EDGE_KINDS: &[EdgeKind] = &[
    EdgeKind::Contains,
    EdgeKind::Calls,
    EdgeKind::Imports,
    EdgeKind::Exports,
    EdgeKind::Extends,
    EdgeKind::Implements,
    EdgeKind::References,
    EdgeKind::TypeOf,
    EdgeKind::Returns,
    EdgeKind::Instantiates,
    EdgeKind::Overrides,
    EdgeKind::Decorates,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Language {
    #[serde(rename = "typescript")]
    TypeScript,
    #[serde(rename = "javascript")]
    JavaScript,
    #[serde(rename = "tsx")]
    Tsx,
    #[serde(rename = "jsx")]
    Jsx,
    #[serde(rename = "python")]
    Python,
    #[serde(rename = "go")]
    Go,
    #[serde(rename = "rust")]
    Rust,
    #[serde(rename = "java")]
    Java,
    #[serde(rename = "c")]
    C,
    #[serde(rename = "cpp")]
    Cpp,
    #[serde(rename = "csharp")]
    CSharp,
    #[serde(rename = "razor")]
    Razor,
    #[serde(rename = "php")]
    Php,
    #[serde(rename = "ruby")]
    Ruby,
    #[serde(rename = "swift")]
    Swift,
    #[serde(rename = "kotlin")]
    Kotlin,
    #[serde(rename = "dart")]
    Dart,
    #[serde(rename = "svelte")]
    Svelte,
    #[serde(rename = "vue")]
    Vue,
    #[serde(rename = "astro")]
    Astro,
    #[serde(rename = "liquid")]
    Liquid,
    #[serde(rename = "pascal")]
    Pascal,
    #[serde(rename = "scala")]
    Scala,
    #[serde(rename = "lua")]
    Lua,
    #[serde(rename = "luau")]
    Luau,
    #[serde(rename = "objc")]
    ObjC,
    #[serde(rename = "r")]
    R,
    #[serde(rename = "yaml")]
    Yaml,
    #[serde(rename = "twig")]
    Twig,
    #[serde(rename = "xml")]
    Xml,
    #[serde(rename = "properties")]
    Properties,
    #[serde(rename = "unknown")]
    Unknown,
}

/// 语言列表同时覆盖 tree-sitter 语言和少量配置/模板格式。
pub const LANGUAGES: &[Language] = &[
    Language::TypeScript,
    Language::JavaScript,
    Language::Tsx,
    Language::Jsx,
    Language::Python,
    Language::Go,
    Language::Rust,
    Language::Java,
    Language::C,
    Language::Cpp,
    Language::CSharp,
    Language::Razor,
    Language::Php,
    Language::Ruby,
    Language::Swift,
    Language::Kotlin,
    Language::Dart,
    Language::Svelte,
    Language::Vue,
    Language::Astro,
    Language::Liquid,
    Language::Pascal,
    Language::Scala,
    Language::Lua,
    Language::Luau,
    Language::ObjC,
    Language::R,
    Language::Yaml,
    Language::Twig,
    Language::Xml,
    Language::Properties,
    Language::Unknown,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Visibility {
    Public,
    Private,
    Protected,
    Internal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EdgeProvenance {
    #[serde(rename = "tree-sitter")]
    TreeSitter,
    #[serde(rename = "scip")]
    Scip,
    #[serde(rename = "heuristic")]
    Heuristic,
}

/// 图节点的最小可索引单元。可选字段保持稀疏，避免不同语言缺失信息时制造假值。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Node {
    pub id: String,
    pub kind: NodeKind,
    pub name: String,
    pub qualified_name: String,
    pub file_path: String,
    pub language: Language,
    pub start_line: LineNumber,
    pub end_line: LineNumber,
    pub start_column: ColumnNumber,
    pub end_column: ColumnNumber,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub docstring: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visibility: Option<Visibility>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_exported: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_async: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_static: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_abstract: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decorators: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub type_parameters: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub return_type: Option<String>,
    pub updated_at: TimestampMs,
}

/// 图边可以来自解析器、resolver 或启发式合成；metadata 承载框架特定上下文。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Edge {
    pub source: String,
    pub target: String,
    pub kind: EdgeKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<LineNumber>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<ColumnNumber>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provenance: Option<EdgeProvenance>,
}

/// 每个文件的索引状态快照，DB 用它判断增量同步和错误展示。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileRecord {
    pub path: String,
    pub content_hash: String,
    pub language: Language,
    pub size: ByteSize,
    pub modified_at: TimestampMs,
    pub indexed_at: TimestampMs,
    pub node_count: Count,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub errors: Option<Vec<ExtractionError>>,
}

/// 单文件抽取结果：已解析出的节点/边，加上后续 resolver 需要处理的未解析引用。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionResult {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub unresolved_references: Vec<UnresolvedReference>,
    pub errors: Vec<ExtractionError>,
    pub duration_ms: DurationMs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExtractionSeverity {
    Error,
    Warning,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionError {
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<LineNumber>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<ColumnNumber>,
    pub severity: ExtractionSeverity,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReferenceKind {
    Contains,
    Calls,
    Imports,
    Exports,
    Extends,
    Implements,
    References,
    TypeOf,
    Returns,
    Instantiates,
    Overrides,
    Decorates,
    FunctionRef,
}

impl From<EdgeKind> for ReferenceKind {
    fn from(kind: EdgeKind) -> Self {
        match kind {
            EdgeKind::Contains => Self::Contains,
            EdgeKind::Calls => Self::Calls,
            EdgeKind::Imports => Self::Imports,
            EdgeKind::Exports => Self::Exports,
            EdgeKind::Extends => Self::Extends,
            EdgeKind::Implements => Self::Implements,
            EdgeKind::References => Self::References,
            EdgeKind::TypeOf => Self::TypeOf,
            EdgeKind::Returns => Self::Returns,
            EdgeKind::Instantiates => Self::Instantiates,
            EdgeKind::Overrides => Self::Overrides,
            EdgeKind::Decorates => Self::Decorates,
        }
    }
}

/// 抽取阶段无法确定目标时先保留引用名和候选信息，后续解析器再补成真实边。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnresolvedReference {
    pub from_node_id: String,
    pub reference_name: String,
    pub reference_kind: ReferenceKind,
    pub line: LineNumber,
    pub column: ColumnNumber,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<Language>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub candidates: Option<Vec<String>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    High,
    Low,
}

/// 子图可能混合精确边和启发式边，confidence 给上层格式化一个整体提示位。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Subgraph {
    pub nodes: HashMap<String, Node>,
    pub edges: Vec<Edge>,
    pub roots: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<Confidence>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TraversalDirection {
    Outgoing,
    Incoming,
    Both,
}

/// TraversalOptions 是图查询的公共过滤器；所有字段可选以便 MCP 工具逐步收窄。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TraversalOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_depth: Option<Count>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edge_kinds: Option<Vec<EdgeKind>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_kinds: Option<Vec<NodeKind>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub direction: Option<TraversalDirection>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<Count>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_start: Option<bool>,
}

/// 搜索选项同时服务全文搜索和符号过滤，include/exclude 保持路径模式语义。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kinds: Option<Vec<NodeKind>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub languages: Option<Vec<Language>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_patterns: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exclude_patterns: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<Count>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<Count>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub case_sensitive: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    pub node: Node,
    pub score: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub highlights: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeEdgeRef {
    pub node: Node,
    pub edge: Edge,
}

/// Context 是围绕单个焦点节点的局部视图，适合符号详情页或 MCP node 输出。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Context {
    pub focal: Node,
    pub ancestors: Vec<Node>,
    pub children: Vec<Node>,
    pub incoming_refs: Vec<NodeEdgeRef>,
    pub outgoing_refs: Vec<NodeEdgeRef>,
    pub types: Vec<Node>,
    pub imports: Vec<Node>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeBlock {
    pub content: String,
    pub file_path: String,
    pub start_line: LineNumber,
    pub end_line: LineNumber,
    pub language: Language,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node: Option<Node>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaVersion {
    pub version: Count,
    pub applied_at: TimestampMs,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphStats {
    pub node_count: Count,
    pub edge_count: Count,
    pub file_count: Count,
    pub nodes_by_kind: HashMap<NodeKind, Count>,
    pub edges_by_kind: HashMap<EdgeKind, Count>,
    pub files_by_language: HashMap<Language, Count>,
    pub db_size_bytes: ByteSize,
    pub last_updated: TimestampMs,
}

/// TaskInput 是 untagged 枚举，兼容“只给查询字符串”和“结构化任务描述”两种调用方。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TaskInput {
    Query(String),
    Details(TaskInputDetails),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskInputDetails {
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ContextFormat {
    Markdown,
    Json,
}

/// 构建上下文的预算选项；默认值由 context 模块决定，这里只定义跨层传输形状。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildContextOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_nodes: Option<Count>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_code_blocks: Option<Count>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_code_block_size: Option<ByteSize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_code: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<ContextFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search_limit: Option<Count>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub traversal_depth: Option<Count>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_score: Option<f64>,
}

/// TaskContext 是给代理消费的聚合结果，包含子图、代码块和摘要统计。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskContext {
    pub query: String,
    pub subgraph: Subgraph,
    pub entry_points: Vec<Node>,
    pub code_blocks: Vec<CodeBlock>,
    pub related_files: Vec<String>,
    pub summary: String,
    pub stats: TaskContextStats,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskContextStats {
    pub node_count: Count,
    pub edge_count: Count,
    pub file_count: Count,
    pub code_block_count: Count,
    pub total_code_size: ByteSize,
}

/// findRelevantContext 旧入口仍依赖这组过滤器，字段命名保持向后兼容。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FindRelevantContextOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search_limit: Option<Count>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub traversal_depth: Option<Count>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_nodes: Option<Count>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_score: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edge_kinds: Option<Vec<EdgeKind>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_kinds: Option<Vec<NodeKind>>,
}
