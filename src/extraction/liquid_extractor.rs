//! Rust translation of `src/extraction/liquid-extractor.ts`.
//!
//! Liquid is intentionally handled as regex/JSON template parsing: section and
//! snippet tags are references to other template files, schema is a constant,
//! and assignments become variable nodes. The regex literals mirror the TS
//! extractor because parity is more valuable than abstraction here.

use std::collections::HashSet;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use regex::Regex;
use serde_json::Value;
use std::sync::LazyLock;

use crate::extraction::tree_sitter_helpers::generate_node_id;
use crate::types::{
    Edge, EdgeKind, ExtractionError, ExtractionResult, Language, Node, NodeKind, ReferenceKind,
    UnresolvedReference,
};

static RENDER_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"\{%[-]?\s*(render|include)\s+['"]([^'"]+)['"]"#).unwrap());
static SECTION_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"\{%[-]?\s*section\s+['"]([^'"]+)['"]"#).unwrap());
static SCHEMA_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)\{%[-]?\s*schema\s*[-]?%\}(.*?)\{%[-]?\s*endschema\s*[-]?%\}").unwrap()
});
static ASSIGN_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\{%[-]?\s*assign\s+(\w+)\s*=").unwrap());

pub struct LiquidExtractor {
    file_path: String,
    source: String,
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    unresolved_references: Vec<UnresolvedReference>,
    errors: Vec<ExtractionError>,
}

impl LiquidExtractor {
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

    /// Extract from Liquid source.
    pub fn extract(mut self) -> ExtractionResult {
        let start = Instant::now();
        let file_node = self.create_file_node();

        // Shopify JSON 模板只表达 section 引用；Liquid 模板才扫描标签、schema 和赋值。
        if self.file_path.ends_with(".json") {
            self.extract_shopify_json_sections(&file_node.id);
        } else {
            self.extract_snippet_references(&file_node.id);
            self.extract_section_references(&file_node.id);
            self.extract_schema(&file_node.id);
            self.extract_assignments(&file_node.id);
        }

        ExtractionResult {
            nodes: self.nodes,
            edges: self.edges,
            unresolved_references: self.unresolved_references,
            errors: self.errors,
            duration_ms: start.elapsed().as_millis() as u64,
        }
    }

    /// Create a file node for the Liquid template.
    fn create_file_node(&mut self) -> Node {
        let lines = self.source.split('\n').collect::<Vec<_>>();
        let id = generate_node_id(&self.file_path, NodeKind::File, &self.file_path, 1);
        let node = Node {
            id,
            kind: NodeKind::File,
            name: basename(&self.file_path),
            qualified_name: self.file_path.clone(),
            file_path: self.file_path.clone(),
            language: Language::Liquid,
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
        self.nodes.push(node.clone());
        node
    }

    /// Shopify OS 2.0 JSON template / section group references.
    fn extract_shopify_json_sections(&mut self, from_node_id: &str) {
        let Ok(parsed) = serde_json::from_str::<Value>(&self.source) else {
            return;
        };
        let Some(sections) = parsed.get("sections").and_then(Value::as_object) else {
            return;
        };

        let mut seen = HashSet::new();
        for section in sections.values() {
            let Some(section_type) = section.get("type").and_then(Value::as_str) else {
                continue;
            };
            if !seen.insert(section_type.to_owned()) {
                continue;
            }
            // JSON 模板的 section type 对应 `sections/<type>.liquid`，没有源码位置时统一记在首行。
            self.unresolved_references.push(UnresolvedReference {
                from_node_id: from_node_id.to_owned(),
                reference_name: format!("sections/{section_type}.liquid"),
                reference_kind: ReferenceKind::References,
                line: 1,
                column: 0,
                file_path: None,
                language: None,
                candidates: None,
            });
        }
    }

    /// Extract `{% render 'snippet' %}` and `{% include 'snippet' %}` references.
    fn extract_snippet_references(&mut self, file_node_id: &str) {
        // render/include 同时建 import 和 component：前者用于依赖解析，后者让模板组件出现在图里。
        let matches = RENDER_REGEX
            .captures_iter(&self.source)
            .filter_map(|caps| {
                let full = caps.get(0)?;
                let tag_type = caps.get(1)?.as_str().to_owned();
                let snippet_name = caps.get(2)?.as_str().to_owned();
                Some((
                    full.start(),
                    full.as_str().to_owned(),
                    tag_type,
                    snippet_name,
                ))
            })
            .collect::<Vec<_>>();

        for (start, full_match, tag_type, snippet_name) in matches {
            let line = self.get_line_number(start);
            let column = start.saturating_sub(self.get_line_start(line));

            let import_node_id =
                generate_node_id(&self.file_path, NodeKind::Import, &snippet_name, line);
            self.nodes.push(Node {
                id: import_node_id.clone(),
                kind: NodeKind::Import,
                name: snippet_name.clone(),
                qualified_name: format!("{}::import:{}", self.file_path, snippet_name),
                file_path: self.file_path.clone(),
                language: Language::Liquid,
                start_line: line as u64,
                end_line: line as u64,
                start_column: column as u64,
                end_column: (column + full_match.len()) as u64,
                docstring: None,
                signature: Some(full_match.clone()),
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
            self.edges
                .push(contains_edge(file_node_id, &import_node_id));

            let node_id = generate_node_id(
                &self.file_path,
                NodeKind::Component,
                &format!("{tag_type}:{snippet_name}"),
                line,
            );
            self.nodes.push(Node {
                id: node_id.clone(),
                kind: NodeKind::Component,
                name: snippet_name.clone(),
                qualified_name: format!("{}::{}:{}", self.file_path, tag_type, snippet_name),
                file_path: self.file_path.clone(),
                language: Language::Liquid,
                start_line: line as u64,
                end_line: line as u64,
                start_column: column as u64,
                end_column: (column + full_match.len()) as u64,
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
            });
            self.edges.push(contains_edge(file_node_id, &node_id));

            self.unresolved_references.push(UnresolvedReference {
                from_node_id: file_node_id.to_owned(),
                reference_name: format!("snippets/{snippet_name}.liquid"),
                reference_kind: ReferenceKind::References,
                line: line as u64,
                column: column as u64,
                file_path: None,
                language: None,
                candidates: None,
            });
        }
    }

    /// Extract `{% section 'name' %}` references.
    fn extract_section_references(&mut self, file_node_id: &str) {
        // section 标签指向 Shopify sections 目录，和 snippet 分开放置便于 resolver 匹配路径。
        let matches = SECTION_REGEX
            .captures_iter(&self.source)
            .filter_map(|caps| {
                let full = caps.get(0)?;
                let section_name = caps.get(1)?.as_str().to_owned();
                Some((full.start(), full.as_str().to_owned(), section_name))
            })
            .collect::<Vec<_>>();

        for (start, full_match, section_name) in matches {
            let line = self.get_line_number(start);
            let column = start.saturating_sub(self.get_line_start(line));

            let import_node_id =
                generate_node_id(&self.file_path, NodeKind::Import, &section_name, line);
            self.nodes.push(Node {
                id: import_node_id.clone(),
                kind: NodeKind::Import,
                name: section_name.clone(),
                qualified_name: format!("{}::import:{}", self.file_path, section_name),
                file_path: self.file_path.clone(),
                language: Language::Liquid,
                start_line: line as u64,
                end_line: line as u64,
                start_column: column as u64,
                end_column: (column + full_match.len()) as u64,
                docstring: None,
                signature: Some(full_match.clone()),
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
            self.edges
                .push(contains_edge(file_node_id, &import_node_id));

            let node_id = generate_node_id(
                &self.file_path,
                NodeKind::Component,
                &format!("section:{section_name}"),
                line,
            );
            self.nodes.push(Node {
                id: node_id.clone(),
                kind: NodeKind::Component,
                name: section_name.clone(),
                qualified_name: format!("{}::section:{}", self.file_path, section_name),
                file_path: self.file_path.clone(),
                language: Language::Liquid,
                start_line: line as u64,
                end_line: line as u64,
                start_column: column as u64,
                end_column: (column + full_match.len()) as u64,
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
            });
            self.edges.push(contains_edge(file_node_id, &node_id));

            self.unresolved_references.push(UnresolvedReference {
                from_node_id: file_node_id.to_owned(),
                reference_name: format!("sections/{section_name}.liquid"),
                reference_kind: ReferenceKind::References,
                line: line as u64,
                column: column as u64,
                file_path: None,
                language: None,
                candidates: None,
            });
        }
    }

    /// Extract `{% schema %}...{% endschema %}` blocks.
    fn extract_schema(&mut self, file_node_id: &str) {
        let matches = SCHEMA_REGEX
            .captures_iter(&self.source)
            .filter_map(|caps| {
                let full = caps.get(0)?;
                let schema_content = caps.get(1).map(|m| m.as_str()).unwrap_or("").to_owned();
                Some((
                    full.start(),
                    full.end(),
                    full.as_str().to_owned(),
                    schema_content,
                ))
            })
            .collect::<Vec<_>>();

        for (start, end, _full_match, schema_content) in matches {
            let start_line = self.get_line_number(start);
            let end_line = self.get_line_number(end);
            let schema_name = parse_schema_name(&schema_content).unwrap_or_else(|| "schema".into());
            // schema JSON 是模板配置常量，名称可能是字符串，也可能是本地化 map。
            let node_id = generate_node_id(
                &self.file_path,
                NodeKind::Constant,
                &format!("schema:{schema_name}"),
                start_line,
            );
            self.nodes.push(Node {
                id: node_id.clone(),
                kind: NodeKind::Constant,
                name: schema_name.clone(),
                qualified_name: format!("{}::schema:{}", self.file_path, schema_name),
                file_path: self.file_path.clone(),
                language: Language::Liquid,
                start_line: start_line as u64,
                end_line: end_line as u64,
                start_column: start.saturating_sub(self.get_line_start(start_line)) as u64,
                end_column: 0,
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
            });
            self.edges.push(contains_edge(file_node_id, &node_id));
        }
    }

    /// Extract `{% assign var = value %}` statements.
    fn extract_assignments(&mut self, file_node_id: &str) {
        let matches = ASSIGN_REGEX
            .captures_iter(&self.source)
            .filter_map(|caps| {
                let full = caps.get(0)?;
                let variable_name = caps.get(1)?.as_str().to_owned();
                Some((full.start(), full.as_str().to_owned(), variable_name))
            })
            .collect::<Vec<_>>();

        for (start, full_match, variable_name) in matches {
            let line = self.get_line_number(start);
            let column = start.saturating_sub(self.get_line_start(line));
            let node_id =
                generate_node_id(&self.file_path, NodeKind::Variable, &variable_name, line);
            self.nodes.push(Node {
                id: node_id.clone(),
                kind: NodeKind::Variable,
                name: variable_name.clone(),
                qualified_name: format!("{}::{}", self.file_path, variable_name),
                file_path: self.file_path.clone(),
                language: Language::Liquid,
                start_line: line as u64,
                end_line: line as u64,
                start_column: column as u64,
                end_column: (column + full_match.len()) as u64,
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
            });
            self.edges.push(contains_edge(file_node_id, &node_id));
        }
    }

    fn get_line_number(&self, index: usize) -> usize {
        self.source[..index.min(self.source.len())]
            .matches('\n')
            .count()
            + 1
    }

    fn get_line_start(&self, line_number: usize) -> usize {
        // Liquid 使用 regex byte offset，按行回算 column 时保持和原字符串同一套索引。
        let mut index = 0;
        for line in self.source.split('\n').take(line_number.saturating_sub(1)) {
            index += line.len() + 1;
        }
        index
    }
}

fn parse_schema_name(schema_content: &str) -> Option<String> {
    let schema_json = serde_json::from_str::<Value>(schema_content).ok()?;
    let name = schema_json.get("name")?;
    if let Some(name) = name.as_str() {
        return Some(name.to_owned());
    }
    if let Some(map) = name.as_object() {
        if let Some(en) = map.get("en").and_then(Value::as_str) {
            return Some(en.to_owned());
        }
        return map.values().find_map(Value::as_str).map(str::to_owned);
    }
    None
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
