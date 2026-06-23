//! Database queries.
//!
//! Prepared statements and row conversions for the CodeGraph SQLite schema.
//! This is a structural translation of `queries.ts`: SQL strings, chunking,
//! cache invalidation, and JSON serialization shapes are intentionally kept
//! close to the TypeScript source.
//!
//! 这个模块是 DB 层的“窄腰”：上层只处理 Rust 类型，子模块只拼 SQL。
//! 因此 enum/JSON/NULL 的互转都集中在这里，避免各查询入口各自发明一套
//! 容错语义。

use std::collections::{HashMap, HashSet, VecDeque};

use crate::extraction::generated_detection::is_generated_file;
use crate::search::query_parser::{bounded_edit_distance, parse_query};
use crate::search::query_utils::{kind_bonus, name_match_bonus, score_path_relevance};
use crate::types::{
    Edge, EdgeKind, EdgeProvenance, FileRecord, GraphStats, Language, Node, NodeKind,
    ReferenceKind, SearchOptions, SearchResult, UnresolvedReference, Visibility,
};

use super::migrations::current_time_millis;
use super::sqlite_adapter::{
    SqliteDatabase, SqliteError, SqliteParams, SqliteResult, SqliteRow, SqliteStatement,
    SqliteValue,
};

mod edges;
mod files;
mod name_search;
mod nodes;
mod routing;
mod search;
mod status;
mod unresolved;

const SQLITE_PARAM_CHUNK_SIZE: usize = 500;

fn is_low_value_file(file_path: &str) -> bool {
    // 用于“推荐入口文件”类查询的降噪，而不是索引过滤：测试和生成文件
    // 仍保留在图里，只在路由/主文件启发式排序时降低影响。
    let lp = file_path.to_lowercase();
    let file_name = lp.rsplit('/').next().unwrap_or(&lp);

    lp.contains("/test/")
        || lp.contains("/tests/")
        || lp.contains("/__test__/")
        || lp.contains("/__tests__/")
        || lp.contains("/spec/")
        || lp.starts_with("test/")
        || lp.starts_with("tests/")
        || lp.starts_with("spec/")
        || file_name.ends_with("_test.go")
        || (file_name.starts_with("test_") && file_name.ends_with(".py"))
        || file_name.ends_with("_test.py")
        || file_name.ends_with("_spec.rb")
        || file_name.ends_with("_test.rb")
        || file_name.ends_with(".test.ts")
        || file_name.ends_with(".test.tsx")
        || file_name.ends_with(".test.js")
        || file_name.ends_with(".test.jsx")
        || file_name.ends_with(".spec.ts")
        || file_name.ends_with(".spec.tsx")
        || file_name.ends_with(".spec.js")
        || file_name.ends_with(".spec.jsx")
        || file_name.ends_with("test.java")
        || file_name.ends_with("tests.java")
        || file_name.ends_with("spec.java")
        || file_name.ends_with("test.kt")
        || file_name.ends_with("tests.kt")
        || file_name.ends_with("spec.kt")
        || file_name.ends_with("test.scala")
        || file_name.ends_with("tests.scala")
        || file_name.ends_with("spec.scala")
        || file_name.ends_with("test.cs")
        || file_name.ends_with("tests.cs")
        || file_name.ends_with("spec.cs")
        || file_name.ends_with("tests.swift")
        || file_name.ends_with("_test.dart")
        || is_generated_file(file_path)
}

fn db_error(message: impl Into<String>) -> SqliteError {
    SqliteError::new(message)
}

fn parse_db_enum<T>(value: String, field: &str) -> SqliteResult<T>
where
    T: serde::de::DeserializeOwned,
{
    serde_json::from_value(serde_json::Value::String(value.clone()))
        .map_err(|error| db_error(format!("invalid {field} value `{value}`: {error}")))
}

fn db_enum_string<T>(value: &T, field: &str) -> SqliteResult<String>
where
    T: serde::Serialize,
{
    match serde_json::to_value(value)
        .map_err(|error| db_error(format!("failed to serialize `{field}` enum: {error}")))?
    {
        serde_json::Value::String(value) => Ok(value),
        other => Err(db_error(format!(
            "serialized `{field}` enum was not a string: {other}"
        ))),
    }
}

fn row_value<'a>(row: &'a SqliteRow, field: &str) -> SqliteResult<&'a SqliteValue> {
    row.get(field)
        .ok_or_else(|| db_error(format!("missing database column `{field}`")))
}

fn row_string(row: &SqliteRow, field: &str) -> SqliteResult<String> {
    row_value(row, field)?
        .clone()
        .into_string_lossy()
        .ok_or_else(|| db_error(format!("database column `{field}` was not text-like")))
}

fn row_optional_string(row: &SqliteRow, field: &str) -> Option<String> {
    row.get(field)
        .cloned()
        .and_then(SqliteValue::into_string_lossy)
        .filter(|value| !value.is_empty())
}

fn row_i64(row: &SqliteRow, field: &str) -> SqliteResult<i64> {
    row_value(row, field)?
        .as_i64()
        .ok_or_else(|| db_error(format!("database column `{field}` was not integer-like")))
}

fn row_u64(row: &SqliteRow, field: &str) -> SqliteResult<u64> {
    let value = row_i64(row, field)?;
    u64::try_from(value).map_err(|_| db_error(format!("database column `{field}` was negative")))
}

fn row_optional_i64(row: &SqliteRow, field: &str) -> Option<i64> {
    row.get(field).and_then(SqliteValue::as_i64)
}

fn row_optional_u64(row: &SqliteRow, field: &str) -> Option<u64> {
    row_optional_i64(row, field).and_then(|value| u64::try_from(value).ok())
}

fn row_bool(row: &SqliteRow, field: &str) -> SqliteResult<bool> {
    row_value(row, field)?
        .as_bool()
        .ok_or_else(|| db_error(format!("database column `{field}` was not bool-like")))
}

fn parse_json_option<T>(raw: Option<String>, _field: &str) -> SqliteResult<Option<T>>
where
    T: serde::de::DeserializeOwned,
{
    // 历史库里 JSON 字段可能来自旧版 extractor 或手工调试数据。
    // 解析失败时丢弃该可选字段，而不是让整个节点/边不可读取。
    match raw {
        Some(value) => Ok(serde_json::from_str::<T>(&value).ok()),
        None => Ok(None),
    }
}

fn json_text<T: serde::Serialize>(value: &T, field: &str) -> SqliteResult<SqliteValue> {
    serde_json::to_string(value)
        .map(SqliteValue::from)
        .map_err(|error| db_error(format!("failed to serialize `{field}` JSON: {error}")))
}

fn json_text_option<T: serde::Serialize>(
    value: Option<&T>,
    field: &str,
) -> SqliteResult<SqliteValue> {
    match value {
        Some(value) => json_text(value, field),
        None => Ok(SqliteValue::Null),
    }
}

fn named_params(pairs: Vec<(&str, SqliteValue)>) -> SqliteParams {
    SqliteParams::named(
        pairs
            .into_iter()
            .map(|(key, value)| (key.to_string(), value))
            .collect(),
    )
}

fn positional<T>(values: Vec<T>) -> SqliteParams
where
    T: Into<SqliteValue>,
{
    SqliteParams::positional(values.into_iter().map(Into::into).collect())
}

fn placeholders(count: usize) -> String {
    std::iter::repeat_n("?", count)
        .collect::<Vec<_>>()
        .join(",")
}

fn row_to_node(row: &SqliteRow) -> SqliteResult<Node> {
    // 行转换是 schema 与公共类型之间的唯一强校验点：必填列缺失会报错，
    // 可选 JSON/文本字段则按历史兼容规则降级为 None。
    Ok(Node {
        id: row_string(row, "id")?,
        kind: parse_db_enum(row_string(row, "kind")?, "kind")?,
        name: row_string(row, "name")?,
        qualified_name: row_string(row, "qualified_name")?,
        file_path: row_string(row, "file_path")?,
        language: parse_db_enum(row_string(row, "language")?, "language")?,
        start_line: row_u64(row, "start_line")?,
        end_line: row_u64(row, "end_line")?,
        start_column: row_u64(row, "start_column")?,
        end_column: row_u64(row, "end_column")?,
        docstring: row_optional_string(row, "docstring"),
        signature: row_optional_string(row, "signature"),
        visibility: row_optional_string(row, "visibility")
            .map(|visibility| parse_db_enum::<Visibility>(visibility, "visibility"))
            .transpose()?,
        is_exported: Some(row_bool(row, "is_exported")?),
        is_async: Some(row_bool(row, "is_async")?),
        is_static: Some(row_bool(row, "is_static")?),
        is_abstract: Some(row_bool(row, "is_abstract")?),
        decorators: parse_json_option(row_optional_string(row, "decorators"), "decorators")?,
        type_parameters: parse_json_option(
            row_optional_string(row, "type_parameters"),
            "type_parameters",
        )?,
        return_type: row_optional_string(row, "return_type"),
        updated_at: row_i64(row, "updated_at")?,
    })
}

fn row_to_edge(row: &SqliteRow) -> SqliteResult<Edge> {
    Ok(Edge {
        source: row_string(row, "source")?,
        target: row_string(row, "target")?,
        kind: parse_db_enum(row_string(row, "kind")?, "kind")?,
        metadata: parse_json_option(row_optional_string(row, "metadata"), "metadata")?,
        line: row_optional_u64(row, "line"),
        column: row_optional_u64(row, "col"),
        provenance: row_optional_string(row, "provenance")
            .map(|provenance| parse_db_enum::<EdgeProvenance>(provenance, "provenance"))
            .transpose()?,
    })
}

fn row_to_file_record(row: &SqliteRow) -> SqliteResult<FileRecord> {
    Ok(FileRecord {
        path: row_string(row, "path")?,
        content_hash: row_string(row, "content_hash")?,
        language: parse_db_enum(row_string(row, "language")?, "language")?,
        size: row_u64(row, "size")?,
        modified_at: row_i64(row, "modified_at")?,
        indexed_at: row_i64(row, "indexed_at")?,
        node_count: row_u64(row, "node_count")?,
        errors: parse_json_option(row_optional_string(row, "errors"), "errors")?,
    })
}

fn row_to_unresolved_reference(row: &SqliteRow) -> SqliteResult<UnresolvedReference> {
    Ok(UnresolvedReference {
        from_node_id: row_string(row, "from_node_id")?,
        reference_name: row_string(row, "reference_name")?,
        reference_kind: parse_db_enum::<ReferenceKind>(
            row_string(row, "reference_kind")?,
            "reference_kind",
        )?,
        line: row_u64(row, "line")?,
        column: row_u64(row, "col")?,
        file_path: row_optional_string(row, "file_path"),
        language: row_optional_string(row, "language")
            .map(|language| parse_db_enum(language, "language"))
            .transpose()?,
        candidates: parse_json_option(row_optional_string(row, "candidates"), "candidates")?,
    })
}

fn map_rows<T>(
    rows: Vec<SqliteRow>,
    mapper: impl Fn(&SqliteRow) -> SqliteResult<T>,
) -> SqliteResult<Vec<T>> {
    rows.iter().map(mapper).collect()
}

#[derive(Default)]
struct QueryStatements {
    // 只缓存固定 SQL 的 prepared statement。带动态 IN 列表或可选过滤器的查询
    // 每次重新 prepare，避免把参数个数不同的 SQL 塞进同一个槽位。
    insert_node: Option<Box<dyn SqliteStatement>>,
    update_node: Option<Box<dyn SqliteStatement>>,
    delete_node: Option<Box<dyn SqliteStatement>>,
    delete_nodes_by_file: Option<Box<dyn SqliteStatement>>,
    get_node_by_id: Option<Box<dyn SqliteStatement>>,
    get_nodes_by_file: Option<Box<dyn SqliteStatement>>,
    get_nodes_by_kind: Option<Box<dyn SqliteStatement>>,
    insert_edge: Option<Box<dyn SqliteStatement>>,
    upsert_file: Option<Box<dyn SqliteStatement>>,
    delete_edges_by_source: Option<Box<dyn SqliteStatement>>,
    get_edges_by_source: Option<Box<dyn SqliteStatement>>,
    get_edges_by_target: Option<Box<dyn SqliteStatement>>,
    delete_file: Option<Box<dyn SqliteStatement>>,
    get_file_by_path: Option<Box<dyn SqliteStatement>>,
    get_all_files: Option<Box<dyn SqliteStatement>>,
    insert_unresolved: Option<Box<dyn SqliteStatement>>,
    delete_unresolved_by_node: Option<Box<dyn SqliteStatement>>,
    get_unresolved_by_name: Option<Box<dyn SqliteStatement>>,
    get_nodes_by_name: Option<Box<dyn SqliteStatement>>,
    get_nodes_by_qualified_name_exact: Option<Box<dyn SqliteStatement>>,
    get_nodes_by_lower_name: Option<Box<dyn SqliteStatement>>,
    get_unresolved_count: Option<Box<dyn SqliteStatement>>,
    get_unresolved_batch: Option<Box<dyn SqliteStatement>>,
    get_all_file_paths: Option<Box<dyn SqliteStatement>>,
    get_all_node_names: Option<Box<dyn SqliteStatement>>,
    get_dominant_file: Option<Box<dyn SqliteStatement>>,
    get_top_route_file: Option<Box<dyn SqliteStatement>>,
    get_routing_manifest: Option<Box<dyn SqliteStatement>>,
}

/// Densest internal-edge file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DominantFile {
    pub file_path: String,
    pub edge_count: i64,
    pub next_edge_count: i64,
}

/// File holding the densest concentration of route nodes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopRouteFile {
    pub file_path: String,
    pub route_count: i64,
    pub total_routes: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutingManifestEntry {
    pub url: String,
    pub handler: String,
    pub handler_file: String,
    pub handler_line: i64,
    pub handler_kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutingManifest {
    pub entries: Vec<RoutingManifestEntry>,
    pub top_handler_file: Option<String>,
    pub top_handler_file_count: i64,
    pub total_routes: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodeEdgeCount {
    pub nodes: u64,
    pub edges: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedReferenceKey {
    pub from_node_id: String,
    pub reference_name: String,
    pub reference_kind: String,
}

/// Query builder for the knowledge graph database.
pub struct QueryBuilder<'db> {
    db: &'db mut dyn SqliteDatabase,
    project_name_tokens: HashSet<String>,
    // 高频图遍历会反复按 id 取节点；这个小型 LRU 避免在一次 explore/node
    // 调用里对同一批节点重复走 SQLite。
    node_cache: HashMap<String, Node>,
    node_cache_order: VecDeque<String>,
    max_cache_size: usize,
    stmts: QueryStatements,
}

impl<'db> QueryBuilder<'db> {
    pub fn new(db: &'db mut dyn SqliteDatabase) -> Self {
        Self {
            db,
            project_name_tokens: HashSet::new(),
            node_cache: HashMap::new(),
            node_cache_order: VecDeque::new(),
            max_cache_size: 1000,
            stmts: QueryStatements::default(),
        }
    }

    /// Set normalized project-name tokens used to down-weight path scoring.
    pub fn set_project_name_tokens(&mut self, tokens: HashSet<String>) {
        self.project_name_tokens = tokens;
    }

    /// The normalized project-name tokens.
    pub fn get_project_name_tokens(&self) -> &HashSet<String> {
        &self.project_name_tokens
    }

    fn prepare_cached<'slot>(
        db: &mut dyn SqliteDatabase,
        slot: &'slot mut Option<Box<dyn SqliteStatement>>,
        sql: &str,
    ) -> SqliteResult<&'slot mut dyn SqliteStatement> {
        if slot.is_none() {
            *slot = Some(db.prepare(sql)?);
        }
        Ok(slot.as_deref_mut().expect("statement was just prepared"))
    }

    fn touch_cached_node(&mut self, id: &str) {
        self.node_cache_order.retain(|cached_id| cached_id != id);
        self.node_cache_order.push_back(id.to_string());
    }

    fn cache_node(&mut self, node: Node) {
        if self.node_cache.contains_key(&node.id) {
            self.touch_cached_node(&node.id);
            self.node_cache.insert(node.id.clone(), node);
            return;
        }

        while self.node_cache.len() >= self.max_cache_size {
            // `node_cache_order` 只保存 id，驱逐时再清 map；retain/touch 的成本
            // 对 1000 项上限可接受，换来实现简单且不引入额外依赖。
            if let Some(first_key) = self.node_cache_order.pop_front() {
                self.node_cache.remove(&first_key);
            } else {
                break;
            }
        }
        self.node_cache_order.push_back(node.id.clone());
        self.node_cache.insert(node.id.clone(), node);
    }

    fn remove_cached_node(&mut self, id: &str) {
        self.node_cache.remove(id);
        self.node_cache_order.retain(|cached_id| cached_id != id);
    }
}

fn append_kind_language_filters(
    sql: &mut String,
    params: &mut Vec<SqliteValue>,
    kind_column: &str,
    language_column: &str,
    kinds: &Option<Vec<NodeKind>>,
    languages: &Option<Vec<Language>>,
) -> SqliteResult<()> {
    // 过滤器直接拼列名但不拼用户值；调用点只传固定列名，实际 kind/language
    // 仍走绑定参数，避免动态 SQL 变成注入面。
    if let Some(kinds) = kinds
        && !kinds.is_empty()
    {
        sql.push_str(&format!(
            " AND {kind_column} IN ({})",
            placeholders(kinds.len())
        ));
        params.extend(
            kinds
                .iter()
                .map(|kind| db_enum_string(kind, "kind").map(SqliteValue::from))
                .collect::<SqliteResult<Vec<_>>>()?,
        );
    }
    if let Some(languages) = languages
        && !languages.is_empty()
    {
        sql.push_str(&format!(
            " AND {language_column} IN ({})",
            placeholders(languages.len())
        ));
        params.extend(
            languages
                .iter()
                .map(|language| db_enum_string(language, "language").map(SqliteValue::from))
                .collect::<SqliteResult<Vec<_>>>()?,
        );
    }
    Ok(())
}
