//! Rust translation of `src/extraction/svelte-extractor.ts`.
//!
//! Svelte is mixed-language markup. This extractor keeps the TS behavior:
//! create one component node for the file, delegate `<script>` content to the
//! generic extractor, and regex-scan template expressions/tags for references.
//!
//! 这里的目标不是完整理解 Svelte 编译语义，而是把单文件组件映射成
//! RustCodeGraph 可解析的最小图：文件级 component 节点承载模板引用，
//! `<script>` 里的 JS/TS 仍交给通用 tree-sitter 抽取器处理。

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

const SVELTE_RUNES: &[&str] = &[
    "$props",
    "$state",
    "$derived",
    "$effect",
    "$bindable",
    "$inspect",
    "$host",
    "$snippet",
];

static SCRIPT_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?s)<script(\s[^>]*)?>(.*?)</script>").unwrap());
static LANG_TS_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"lang\s*=\s*["'](ts|typescript)["']"#).unwrap());
static CONTEXT_MODULE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"context\s*=\s*["']module["']"#).unwrap());
static BLOCK_OPEN_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<(script|style)(\s[^>]*)?>").unwrap());
static TEMPLATE_EXPR_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\{([^}#/:@][^}]*)\}").unwrap());
static CALL_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b([a-zA-Z_$][A-Za-z0-9_$.]*)\s*\(").unwrap());
static COMPONENT_TAG_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<([A-Z][a-zA-Z0-9_$]*)\b").unwrap());

#[derive(Debug, Clone)]
struct ScriptBlock {
    content: String,
    // 嵌入脚本被单独送进 JS/TS 解析器，合并回来时必须用这个偏移还原到
    // `.svelte` 原文件行号，否则 MCP 返回的源码片段会错位。
    start_line: usize,
    is_module: bool,
    is_type_script: bool,
}

pub struct SvelteExtractor {
    file_path: String,
    source: String,
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    unresolved_references: Vec<UnresolvedReference>,
    errors: Vec<ExtractionError>,
}

impl SvelteExtractor {
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

    /// Extract from Svelte source.
    pub fn extract(mut self) -> ExtractionResult {
        let start = Instant::now();

        // Svelte 模板本身没有通用 tree-sitter 抽取器，因此先建立一个
        // component 作为模板和脚本节点的共同父节点，再把脚本结果并回图中。
        let component_node = self.create_component_node();
        let script_blocks = self.extract_script_blocks();
        for block in &script_blocks {
            self.process_script_block(block, &component_node.id);
        }
        self.extract_template_calls(&component_node.id, &script_blocks);
        self.extract_template_components(&component_node.id);
        self.unresolved_references
            .retain(|reference| !SVELTE_RUNES.contains(&reference.reference_name.as_str()));

        ExtractionResult {
            nodes: self.nodes,
            edges: self.edges,
            unresolved_references: self.unresolved_references,
            errors: self.errors,
            duration_ms: start.elapsed().as_millis() as u64,
        }
    }

    /// Create a component node for the `.svelte` file.
    fn create_component_node(&mut self) -> Node {
        let lines = self.source.split('\n').collect::<Vec<_>>();
        let file_name = basename(&self.file_path);
        let component_name = file_name
            .strip_suffix(".svelte")
            .unwrap_or(&file_name)
            .to_owned();
        let id = generate_node_id(&self.file_path, NodeKind::Component, &component_name, 1);
        let node = Node {
            id,
            kind: NodeKind::Component,
            name: component_name.clone(),
            qualified_name: format!("{}::{}", self.file_path, component_name),
            file_path: self.file_path.clone(),
            language: Language::Svelte,
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

    /// Extract `<script>` blocks from the Svelte source.
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
                    is_module: CONTEXT_MODULE_REGEX.is_match(attrs),
                    is_type_script: LANG_TS_REGEX.is_match(attrs),
                })
            })
            .collect()
    }

    /// Process a script block by delegating to the generic extractor.
    fn process_script_block(&mut self, block: &ScriptBlock, component_node_id: &str) {
        let _is_module = block.is_module;
        let script_language = if block.is_type_script {
            Language::TypeScript
        } else {
            Language::JavaScript
        };

        if !is_language_supported(script_language) {
            self.errors.push(ExtractionError {
                message: format!(
                    "Parser for {} not available, cannot parse Svelte script block",
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
        self.merge_embedded_result(
            result,
            component_node_id,
            block.start_line,
            Language::Svelte,
        );
    }

    fn merge_embedded_result(
        &mut self,
        result: ExtractionResult,
        component_node_id: &str,
        line_offset: usize,
        language: Language,
    ) {
        // 子解析器看到的是纯脚本文本，所有节点、引用和错误的坐标都需要
        // 平移回原 SFC；同时把 language 设为 Svelte，便于上层按文件类型展示。
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

    /// Extract function calls from Svelte template expressions.
    fn extract_template_calls(&mut self, component_node_id: &str, _script_blocks: &[ScriptBlock]) {
        let covered_ranges = covered_tag_ranges(&self.source);
        let lines = self.source.split('\n').collect::<Vec<_>>();

        for (line_idx, line) in lines.iter().enumerate() {
            // `<script>` / `<style>` 内部已经由其它路径处理或应被忽略；模板扫描
            // 只看剩余文本，避免把 CSS 函数或脚本调用重复记成 component 调用。
            if covered_ranges
                .iter()
                .any(|(start, end)| line_idx >= *start && line_idx <= *end)
            {
                continue;
            }

            for expr_match in TEMPLATE_EXPR_REGEX.captures_iter(line) {
                let Some(full) = expr_match.get(0) else {
                    continue;
                };
                let Some(expr) = expr_match.get(1).map(|m| m.as_str()) else {
                    continue;
                };
                for call_match in CALL_REGEX.captures_iter(expr) {
                    let Some(call_full) = call_match.get(0) else {
                        continue;
                    };
                    let Some(callee_name) = call_match.get(1).map(|m| m.as_str()) else {
                        continue;
                    };
                    if SVELTE_RUNES.contains(&callee_name)
                        || matches!(callee_name, "if" | "else" | "each" | "await")
                    {
                        continue;
                    }
                    self.unresolved_references.push(UnresolvedReference {
                        from_node_id: component_node_id.to_owned(),
                        reference_name: callee_name.to_owned(),
                        reference_kind: ReferenceKind::Calls,
                        line: (line_idx + 1) as u64,
                        column: (full.start() + call_full.start()) as u64,
                        file_path: Some(self.file_path.clone()),
                        language: Some(Language::Svelte),
                        candidates: None,
                    });
                }
            }
        }
    }

    /// Extract component usages from the Svelte template.
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
            for caps in COMPONENT_TAG_REGEX.captures_iter(line) {
                let Some(full) = caps.get(0) else {
                    continue;
                };
                let Some(component_name) = caps.get(1).map(|m| m.as_str()) else {
                    continue;
                };
                self.unresolved_references.push(UnresolvedReference {
                    from_node_id: component_node_id.to_owned(),
                    reference_name: component_name.to_owned(),
                    reference_kind: ReferenceKind::References,
                    line: (line_idx + 1) as u64,
                    column: (full.start() + 1) as u64,
                    file_path: Some(self.file_path.clone()),
                    language: Some(Language::Svelte),
                    candidates: None,
                });
            }
        }
    }
}

fn covered_tag_ranges(source: &str) -> Vec<(usize, usize)> {
    // 简单标签范围足够服务“跳过已覆盖区域”的目的；这里避免引入完整 HTML
    // parser，是为了让 SFC 抽取保持轻量并与 TS 版本的启发式一致。
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
