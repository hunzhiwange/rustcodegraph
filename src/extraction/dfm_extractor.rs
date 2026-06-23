//! Rust translation of `src/extraction/dfm-extractor.ts`.
//!
//! DFM/FMX files use a small text format (`object`/`end` blocks), so this
//! extractor intentionally keeps the TypeScript regex parsing model instead of
//! trying to route the file through a generic tree-sitter adapter.
//!
//! Delphi 表单文件主要提供“组件树”和“事件处理器名字”。这些名字会在
//! Pascal resolver 阶段和 `.pas` 里的方法定义连接起来。

use std::time::{Instant, SystemTime, UNIX_EPOCH};

use regex::Regex;
use std::sync::LazyLock;

use crate::extraction::tree_sitter_helpers::generate_node_id;
use crate::types::{
    Edge, EdgeKind, ExtractionError, ExtractionResult, Language, Node, NodeKind, ReferenceKind,
    UnresolvedReference,
};

static OBJECT_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*(object|inherited|inline)\s+(\w+)\s*:\s*(\w+)").unwrap());
static EVENT_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*(On\w+)\s*=\s*(\w+)\s*$").unwrap());
static END_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*end\s*$").unwrap());
static MULTI_LINE_START: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"=\s*\(\s*$").unwrap());
static MULTI_LINE_ITEM_START: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"=\s*<\s*$").unwrap());

pub struct DfmExtractor {
    file_path: String,
    source: String,
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    unresolved_references: Vec<UnresolvedReference>,
    errors: Vec<ExtractionError>,
}

impl DfmExtractor {
    pub fn new(file_path: impl Into<String>, source: impl Into<String>) -> Self {
        Self {
            file_path: file_path.into(),
            source: source.into(),
            nodes: Vec::new(),
            edges: Vec::new(),
            unresolved_references: Vec::new(),
            errors: Vec::new(),
        }
    }

    /// Extract components and event handler references from DFM/FMX source.
    pub fn extract(mut self) -> ExtractionResult {
        let start = Instant::now();
        let file_node = self.create_file_node();
        self.parse_components(&file_node.id);

        ExtractionResult {
            nodes: self.nodes,
            edges: self.edges,
            unresolved_references: self.unresolved_references,
            errors: self.errors,
            duration_ms: start.elapsed().as_millis() as u64,
        }
    }

    /// Create a file node for the DFM form file.
    fn create_file_node(&mut self) -> Node {
        let lines = self.source.split('\n').collect::<Vec<_>>();
        let id = generate_node_id(&self.file_path, NodeKind::File, &self.file_path, 1);
        let file_node = Node {
            id,
            kind: NodeKind::File,
            name: basename(&self.file_path),
            qualified_name: self.file_path.clone(),
            file_path: self.file_path.clone(),
            language: Language::Pascal,
            start_line: 1,
            end_line: lines.len() as u64,
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
        self.nodes.push(file_node.clone());
        file_node
    }

    /// Parse `object`/`end` blocks and extract components plus event handlers.
    fn parse_components(&mut self, file_node_id: &str) {
        let lines = self.source.split('\n').collect::<Vec<_>>();
        // stack 反映 DFM 的 object/end 嵌套；contains 边按当前栈顶连接，
        // 这样面板、按钮等层级能被图查询还原。
        let mut stack = vec![file_node_id.to_owned()];
        let mut in_multi_line = false;
        let mut multi_line_end_char = ')';

        for (idx, line) in lines.iter().enumerate() {
            let line_num = idx + 1;

            if in_multi_line {
                if line.trim_end().ends_with(multi_line_end_char) {
                    in_multi_line = false;
                }
                continue;
            }
            // Items.Strings = (...) 或 Collection = <...> 内部可能包含任意文本，
            // 不应把里面的 `object`、`OnClick =` 当成真实组件结构。
            if MULTI_LINE_START.is_match(line) {
                in_multi_line = true;
                multi_line_end_char = ')';
                continue;
            }
            if MULTI_LINE_ITEM_START.is_match(line) {
                in_multi_line = true;
                multi_line_end_char = '>';
                continue;
            }

            if let Some(caps) = OBJECT_PATTERN.captures(line) {
                let name = caps.get(2).map(|m| m.as_str()).unwrap_or_default();
                let type_name = caps.get(3).map(|m| m.as_str()).unwrap_or_default();
                let node_id =
                    generate_node_id(&self.file_path, NodeKind::Component, name, line_num);
                self.nodes.push(Node {
                    id: node_id.clone(),
                    kind: NodeKind::Component,
                    name: name.to_owned(),
                    qualified_name: format!("{}#{}", self.file_path, name),
                    file_path: self.file_path.clone(),
                    language: Language::Pascal,
                    start_line: line_num as u64,
                    end_line: line_num as u64,
                    start_column: 0,
                    end_column: line.len() as u64,
                    docstring: None,
                    signature: Some(type_name.to_owned()),
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
                self.edges.push(Edge {
                    source: stack
                        .last()
                        .cloned()
                        .unwrap_or_else(|| file_node_id.to_owned()),
                    target: node_id.clone(),
                    kind: EdgeKind::Contains,
                    metadata: None,
                    line: None,
                    column: None,
                    provenance: None,
                });
                stack.push(node_id);
                continue;
            }

            if let Some(caps) = EVENT_PATTERN.captures(line) {
                let method_name = caps.get(2).map(|m| m.as_str()).unwrap_or_default();
                // 事件处理器只是名字引用，目标方法通常在同名单元的 Pascal 源里；
                // 这里先留下 unresolved ref，让后续解析阶段跨文件连接。
                self.unresolved_references.push(UnresolvedReference {
                    from_node_id: stack
                        .last()
                        .cloned()
                        .unwrap_or_else(|| file_node_id.to_owned()),
                    reference_name: method_name.to_owned(),
                    reference_kind: ReferenceKind::References,
                    line: line_num as u64,
                    column: 0,
                    file_path: None,
                    language: None,
                    candidates: None,
                });
                continue;
            }

            if END_PATTERN.is_match(line) && stack.len() > 1 {
                stack.pop();
            }
        }
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
