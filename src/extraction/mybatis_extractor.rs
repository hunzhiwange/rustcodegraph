//! Rust translation of `src/extraction/mybatis-extractor.ts`.
//!
//! MyBatis mapper XML is not a generic XML symbol language. This extractor
//! keeps the TS regex approach: find `<mapper namespace="...">`, emit method
//! shaped nodes for statement elements, and unresolved references for
//! `<include refid="...">` SQL fragment links.

use std::time::{Instant, SystemTime, UNIX_EPOCH};

use regex::Regex;
use std::sync::LazyLock;

use crate::extraction::tree_sitter_helpers::generate_node_id;
use crate::types::{
    Edge, EdgeKind, ExtractionError, ExtractionResult, Language, Node, NodeKind, ReferenceKind,
    UnresolvedReference,
};

static MAPPER_OPEN_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<mapper\b([^>]*)>").unwrap());
static NAMESPACE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"\bnamespace\s*=\s*"([^"]+)""#).unwrap());
static STMT_OPEN_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<(select|insert|update|delete|sql)\b([^>]*)>").unwrap());
static ID_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"\bid\s*=\s*"([^"]+)""#).unwrap());
static INCLUDE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"<include\b[^>]*\brefid\s*=\s*"([^"]+)""#).unwrap());
static RESULT_TYPE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"\bresultType\s*=\s*"([^"]+)""#).unwrap());
static PARAMETER_TYPE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"\bparameterType\s*=\s*"([^"]+)""#).unwrap());
static TAG_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<[^>]+>").unwrap());
static WHITESPACE_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").unwrap());

pub struct MyBatisExtractor {
    file_path: String,
    source: String,
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    unresolved_references: Vec<UnresolvedReference>,
    errors: Vec<ExtractionError>,
    line_starts: Vec<usize>,
}

struct MapperRoot {
    namespace: String,
    body_start: usize,
    body_end: usize,
}

impl MyBatisExtractor {
    pub fn new(file_path: impl Into<String>, source: impl Into<String>) -> Self {
        let mut extractor = Self {
            file_path: file_path.into(),
            source: source.into(),
            nodes: Vec::new(),
            edges: Vec::new(),
            unresolved_references: Vec::new(),
            errors: Vec::new(),
            line_starts: Vec::new(),
        };
        extractor.compute_line_starts();
        extractor
    }

    pub fn extract(mut self) -> ExtractionResult {
        let start = Instant::now();
        let file_node = self.create_file_node();

        if let Some(mapper) = self.find_mapper_root() {
            self.extract_mapper(
                &file_node.id,
                &mapper.namespace,
                mapper.body_start,
                mapper.body_end,
            );
        }

        ExtractionResult {
            nodes: self.nodes,
            edges: self.edges,
            unresolved_references: self.unresolved_references,
            errors: self.errors,
            duration_ms: start.elapsed().as_millis() as u64,
        }
    }

    fn create_file_node(&mut self) -> Node {
        let lines = self.source.split('\n').collect::<Vec<_>>();
        let id = generate_node_id(&self.file_path, NodeKind::File, &self.file_path, 1);
        let node = Node {
            id,
            kind: NodeKind::File,
            name: basename(&self.file_path),
            qualified_name: self.file_path.clone(),
            file_path: self.file_path.clone(),
            language: Language::Xml,
            start_line: 1,
            end_line: lines.len().max(1) as u64,
            start_column: 0,
            end_column: lines.last().map_or(0, |line| line.len() as u64),
            docstring: None,
            signature: None,
            visibility: None,
            is_exported: None,
            is_async: None,
            is_static: None,
            is_abstract: None,
            decorators: None,
            type_parameters: None,
            return_type: None,
            updated_at: now_ms(),
        };
        self.nodes.push(node.clone());
        node
    }

    /// Find the `<mapper namespace="X">` opening tag and mapper body bounds.
    fn find_mapper_root(&self) -> Option<MapperRoot> {
        let open = MAPPER_OPEN_REGEX.captures(&self.source)?;
        let open_match = open.get(0)?;
        let attrs = open.get(1).map(|m| m.as_str()).unwrap_or("");
        let namespace = NAMESPACE_REGEX.captures(attrs)?.get(1)?.as_str().to_owned();
        let body_start = open_match.end();
        let body_end = self.source[body_start..]
            .find("</mapper>")
            .map(|idx| body_start + idx)
            .unwrap_or(self.source.len());
        Some(MapperRoot {
            namespace,
            body_start,
            body_end,
        })
    }

    fn extract_mapper(
        &mut self,
        file_node_id: &str,
        namespace: &str,
        body_start: usize,
        body_end: usize,
    ) {
        let body = self.source[body_start..body_end].to_owned();
        let mut search_start = 0usize;

        // MyBatis 的语义节点是 mapper 内的 statement/sql 片段，不是任意 XML tag。
        while let Some(caps) = STMT_OPEN_REGEX.captures(&body[search_start..]) {
            let Some(open_match) = caps.get(0) else {
                break;
            };
            let stmt_start = search_start + open_match.start();
            let stmt_open_end = search_start + open_match.end();
            let elem_type = caps
                .get(1)
                .map(|m| m.as_str())
                .unwrap_or_default()
                .to_owned();
            let attrs = caps.get(2).map(|m| m.as_str()).unwrap_or("").to_owned();
            let close_tag = format!("</{elem_type}>");
            let Some(close_rel) = body[stmt_open_end..].find(&close_tag) else {
                // 遇到不完整标签时跳过当前 opening，避免 regex 扫描卡在同一位置。
                search_start = stmt_open_end;
                continue;
            };
            let elem_body_start = stmt_open_end;
            let elem_body_end = stmt_open_end + close_rel;
            let stmt_end = elem_body_end + close_tag.len();
            let elem_body = body[elem_body_start..elem_body_end].to_owned();

            if let Some(id) = ID_REGEX
                .captures(&attrs)
                .and_then(|id_caps| id_caps.get(1))
                .map(|m| m.as_str().to_owned())
            {
                let absolute_index = body_start + stmt_start;
                let start_line = self.get_line_number(absolute_index);
                let end_line = self.get_line_number(body_start + stmt_end);
                let qualified = format!("{namespace}::{id}");
                let is_sql_fragment = elem_type == "sql";
                // select/insert/update/delete/sql 都建成 Method，qualified name 与 Java mapper 方法保持一致。
                let node_id =
                    generate_node_id(&self.file_path, NodeKind::Method, &qualified, start_line);
                self.nodes.push(Node {
                    id: node_id.clone(),
                    kind: NodeKind::Method,
                    name: id,
                    qualified_name: qualified.clone(),
                    file_path: self.file_path.clone(),
                    language: Language::Xml,
                    start_line: start_line as u64,
                    end_line: end_line as u64,
                    start_column: 0,
                    end_column: 0,
                    docstring: Some(preview_sql(&elem_body)),
                    signature: Some(build_signature(&elem_type, &attrs, is_sql_fragment)),
                    visibility: None,
                    is_exported: None,
                    is_async: None,
                    is_static: None,
                    is_abstract: None,
                    decorators: None,
                    type_parameters: None,
                    return_type: None,
                    updated_at: now_ms(),
                });
                self.edges.push(contains_edge(file_node_id, &node_id));

                for include in INCLUDE_REGEX.captures_iter(&elem_body) {
                    let Some(full) = include.get(0) else {
                        continue;
                    };
                    let Some(refid) = include.get(1).map(|m| m.as_str()) else {
                        continue;
                    };
                    let ref_qualified = if refid.contains('.') {
                        refid.replace('.', "::")
                    } else {
                        format!("{namespace}::{refid}")
                    };
                    // include refid 可跨 namespace，也可省略 namespace 指向当前 mapper。
                    let include_offset = body_start + elem_body_start + full.start();
                    let line = self.get_line_number(include_offset);
                    self.unresolved_references.push(UnresolvedReference {
                        from_node_id: node_id.clone(),
                        reference_name: ref_qualified,
                        reference_kind: ReferenceKind::References,
                        line: line as u64,
                        column: 0,
                        file_path: None,
                        language: None,
                        candidates: None,
                    });
                }
            }

            search_start = stmt_end;
        }
    }

    fn compute_line_starts(&mut self) {
        // 预计算行起点让 regex offset 到行号的映射保持 O(log n)，避免 mapper 大文件反复扫描前缀。
        self.line_starts = vec![0];
        for (idx, byte) in self.source.bytes().enumerate() {
            if byte == b'\n' {
                self.line_starts.push(idx + 1);
            }
        }
    }

    fn get_line_number(&self, offset: usize) -> usize {
        let mut lo = 0usize;
        let mut hi = self.line_starts.len().saturating_sub(1);
        while lo < hi {
            let mid = (lo + hi + 1) >> 1;
            if self.line_starts[mid] <= offset {
                lo = mid;
            } else {
                hi = mid - 1;
            }
        }
        lo + 1
    }
}

fn build_signature(elem_type: &str, attrs: &str, is_sql_fragment: bool) -> String {
    if is_sql_fragment {
        return "<sql>".to_owned();
    }
    let mut parts = vec![elem_type.to_ascii_uppercase()];
    if let Some(param) = PARAMETER_TYPE_REGEX
        .captures(attrs)
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str())
    {
        parts.push(format!("param={param}"));
    }
    if let Some(result) = RESULT_TYPE_REGEX
        .captures(attrs)
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str())
    {
        parts.push(format!("result={result}"));
    }
    parts.join(" ")
}

fn preview_sql(body: &str) -> String {
    // docstring 只保留压缩后的 SQL 预览，避免把整段动态 XML 塞进节点摘要。
    let without_tags = TAG_REGEX.replace_all(body, " ");
    let compact = WHITESPACE_REGEX.replace_all(&without_tags, " ");
    compact.trim().chars().take(200).collect()
}

fn contains_edge(source: &str, target: &str) -> Edge {
    Edge {
        source: source.to_owned(),
        target: target.to_owned(),
        kind: EdgeKind::Contains,
        metadata: None,
        line: None,
        column: None,
        provenance: None,
    }
}

fn basename(path: &str) -> String {
    path.rsplit(['/', '\\']).next().unwrap_or(path).to_owned()
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}
