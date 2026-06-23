//! Rust translation of `src/extraction/vue-extractor.ts`.
//!
//! Vue SFCs are handled as component markup with embedded JS/TS. The extractor
//! delegates `<script>` blocks and scans template tags, including Vue's
//! kebab-case component syntax.
//!
//! 这里采用轻量 SFC 模型：component 节点代表整个 `.vue` 文件，脚本块委托
//! 通用 JS/TS 抽取器，模板只抽组件引用，避免把 Vue 编译期语义硬塞进解析层。

use std::time::{Instant, SystemTime, UNIX_EPOCH};

use regex::Regex;
use std::sync::LazyLock;

use crate::extraction::grammars::{is_language_supported, language_key};
use crate::extraction::tree_sitter::extract_from_source;
use crate::extraction::tree_sitter_helpers::generate_node_id;
use crate::types::{
    Edge, EdgeKind, ExtractionError, ExtractionResult, ExtractionSeverity, Language, Node,
    NodeKind, ReferenceKind, UnresolvedReference,
};

const VUE_BUILTIN_COMPONENTS: &[&str] = &[
    "Transition",
    "TransitionGroup",
    "KeepAlive",
    "Suspense",
    "Teleport",
    "Component",
    "Slot",
];

static SCRIPT_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?s)<script(\s[^>]*)?>(.*?)</script>").unwrap());
static LANG_TS_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"lang\s*=\s*["'](ts|typescript)["']"#).unwrap());
static SETUP_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\bsetup\b").unwrap());
static BLOCK_OPEN_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<(script|style)(\s[^>]*)?>").unwrap());
static TAG_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<([A-Za-z][A-Za-z0-9_-]*)\b").unwrap());

#[derive(Debug, Clone)]
struct ScriptBlock {
    content: String,
    // `<script>` 内容被拆出来解析，合并结果时用该偏移修正回原始 SFC 行号。
    start_line: usize,
    is_setup: bool,
    is_type_script: bool,
}

pub struct VueExtractor {
    file_path: String,
    source: String,
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    unresolved_references: Vec<UnresolvedReference>,
    errors: Vec<ExtractionError>,
}

impl VueExtractor {
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

    /// Extract from Vue source.
    pub fn extract(mut self) -> ExtractionResult {
        let start = Instant::now();

        // `<script setup>` 与普通 `<script>` 目前都按 JS/TS 语法抽取；setup 标记
        // 保留在 ScriptBlock 中，方便后续若需要 Vue 作用域语义时扩展。
        let component_node = self.create_component_node();
        let script_blocks = self.extract_script_blocks();
        for block in &script_blocks {
            self.process_script_block(block, &component_node.id);
        }
        self.extract_template_components(&component_node.id);

        ExtractionResult {
            nodes: self.nodes,
            edges: self.edges,
            unresolved_references: self.unresolved_references,
            errors: self.errors,
            duration_ms: start.elapsed().as_millis() as u64,
        }
    }

    /// Create a component node for the `.vue` file.
    fn create_component_node(&mut self) -> Node {
        let lines = self.source.split('\n').collect::<Vec<_>>();
        let file_name = basename(&self.file_path);
        let component_name = file_name
            .strip_suffix(".vue")
            .unwrap_or(&file_name)
            .to_owned();
        let id = generate_node_id(&self.file_path, NodeKind::Component, &component_name, 1);
        let node = Node {
            id,
            kind: NodeKind::Component,
            name: component_name.clone(),
            qualified_name: format!("{}::{}", self.file_path, component_name),
            file_path: self.file_path.clone(),
            language: Language::Vue,
            start_line: 1,
            end_line: lines.len() as u64,
            start_column: 0,
            end_column: lines.last().map_or(0, |line| line.len() as u64),
            docstring: None,
            signature: None,
            visibility: None,
            is_exported: Some(true),
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

    /// Extract `<script>` and `<script setup>` blocks from the Vue source.
    fn extract_script_blocks(&self) -> Vec<ScriptBlock> {
        SCRIPT_REGEX
            .captures_iter(&self.source)
            .filter_map(|caps| {
                let full = caps.get(0)?;
                let attrs = caps.get(1).map(|m| m.as_str()).unwrap_or("");
                let content = caps.get(2).map(|m| m.as_str()).unwrap_or("").to_owned();
                let before_script = &self.source[..full.start()];
                let script_tag_line = before_script.matches('\n').count();
                let opening_tag = full
                    .as_str()
                    .split_once('>')
                    .map(|(tag, _)| format!("{tag}>"))
                    .unwrap_or_default();
                let opening_tag_lines = opening_tag.matches('\n').count();
                Some(ScriptBlock {
                    content,
                    start_line: script_tag_line + opening_tag_lines,
                    is_setup: SETUP_REGEX.is_match(attrs),
                    is_type_script: LANG_TS_REGEX.is_match(attrs),
                })
            })
            .collect()
    }

    /// Process a script block by delegating to the generic extractor.
    fn process_script_block(&mut self, block: &ScriptBlock, component_node_id: &str) {
        let _is_setup = block.is_setup;
        let script_language = if block.is_type_script {
            Language::TypeScript
        } else {
            Language::JavaScript
        };

        if !is_language_supported(script_language) {
            self.errors.push(ExtractionError {
                message: format!(
                    "Parser for {} not available, cannot parse Vue script block",
                    language_key(&script_language)
                ),
                file_path: None,
                line: None,
                column: None,
                severity: ExtractionSeverity::Warning,
                code: None,
            });
            return;
        }

        let result =
            extract_from_source(&self.file_path, &block.content, Some(script_language), None);
        self.merge_embedded_result(result, component_node_id, block.start_line, Language::Vue);
    }

    fn merge_embedded_result(
        &mut self,
        result: ExtractionResult,
        component_node_id: &str,
        line_offset: usize,
        language: Language,
    ) {
        // 子解析器不知道自己来自 SFC，所以这里统一修正行号、文件路径和语言，
        // 并补上 component -> script symbol 的 contains 边。
        for mut node in result.nodes {
            node.start_line += line_offset as u64;
            node.end_line += line_offset as u64;
            node.language = language;
            let node_id = node.id.clone();
            self.nodes.push(node);
            self.edges.push(contains_edge(component_node_id, &node_id));
        }

        for mut edge in result.edges {
            if let Some(line) = edge.line.as_mut() {
                *line += line_offset as u64;
            }
            self.edges.push(edge);
        }

        for mut reference in result.unresolved_references {
            reference.line += line_offset as u64;
            reference.file_path = Some(self.file_path.clone());
            reference.language = Some(language);
            self.unresolved_references.push(reference);
        }

        for mut error in result.errors {
            if let Some(line) = error.line.as_mut() {
                *line += line_offset as u64;
            }
            self.errors.push(error);
        }
    }

    /// Extract component usages from the Vue `<template>`.
    fn extract_template_components(&mut self, component_node_id: &str) {
        let covered_ranges = covered_tag_ranges(&self.source);
        let lines = self.source.split('\n').collect::<Vec<_>>();

        for (line_idx, line) in lines.iter().enumerate() {
            if covered_ranges
                .iter()
                .any(|(start, end)| line_idx >= *start && line_idx <= *end)
            {
                continue;
            }
            for caps in TAG_REGEX.captures_iter(line) {
                let Some(full) = caps.get(0) else {
                    continue;
                };
                let Some(raw) = caps.get(1).map(|m| m.as_str()) else {
                    continue;
                };
                let component_name = if raw.chars().next().is_some_and(|ch| ch.is_ascii_uppercase())
                {
                    raw.to_owned()
                } else if raw.contains('-') {
                    // Vue 模板允许 kebab-case，而脚本导入通常是 PascalCase；引用名
                    // 统一成 PascalCase，交给后续 name matcher 与实际 component 对齐。
                    kebab_to_pascal(raw)
                } else {
                    continue;
                };
                if VUE_BUILTIN_COMPONENTS.contains(&component_name.as_str()) {
                    continue;
                }

                self.unresolved_references.push(UnresolvedReference {
                    from_node_id: component_node_id.to_owned(),
                    reference_name: component_name,
                    reference_kind: ReferenceKind::References,
                    line: (line_idx + 1) as u64,
                    column: (full.start() + 1) as u64,
                    file_path: Some(self.file_path.clone()),
                    language: Some(Language::Vue),
                    candidates: None,
                });
            }
        }
    }
}

/// `my-component` -> `MyComponent` (Vue allows either form in templates).
pub fn kebab_to_pascal(name: &str) -> String {
    name.split('-')
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => format!("{}{}", first.to_ascii_uppercase(), chars.as_str()),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join("")
}

fn covered_tag_ranges(source: &str) -> Vec<(usize, usize)> {
    // 只需要排除 script/style 的行范围，使用正则扫描比完整模板解析更稳定，
    // 也避免把 CSS 选择器或脚本里的标签字符串误当成组件引用。
    let mut ranges = Vec::new();
    let mut search_start = 0usize;
    while let Some(caps) = BLOCK_OPEN_REGEX.captures(&source[search_start..]) {
        let Some(open) = caps.get(0) else {
            break;
        };
        let Some(tag) = caps.get(1).map(|m| m.as_str()) else {
            break;
        };
        let start = search_start + open.start();
        let open_end = search_start + open.end();
        let close_tag = format!("</{tag}>");
        let end = source[open_end..]
            .find(&close_tag)
            .map(|idx| open_end + idx + close_tag.len())
            .unwrap_or(open_end);
        let start_line = source[..start].matches('\n').count();
        let end_line = start_line + source[start..end].matches('\n').count();
        ranges.push((start_line, end_line));
        search_start = end;
    }
    ranges
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
