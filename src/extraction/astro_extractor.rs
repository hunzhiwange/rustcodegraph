//! Rust translation of `src/extraction/astro-extractor.ts`.
//!
//! Astro files combine TS frontmatter, client `<script>` blocks, and JSX-like
//! markup. The extractor keeps those phases separate and delegates embedded TS
//! rather than treating Astro as a generic tree-sitter language.
//!
//! 这个抽取器只负责 Astro 外壳：组件节点、frontmatter/script 的行号重映射，
//! 以及模板里的调用/组件引用线索。真正的 TS/JS 语义仍交给 tree-sitter 抽取器。

use std::time::{Instant, SystemTime, UNIX_EPOCH};

use regex::Regex;
use std::sync::LazyLock;

use crate::extraction::grammars::is_language_supported;
use crate::extraction::tree_sitter::extract_from_source;
use crate::extraction::tree_sitter_helpers::generate_node_id;
use crate::types::{
    Edge, EdgeKind, ExtractionError, ExtractionResult, ExtractionSeverity, Language, Node,
    NodeKind, ReferenceKind, UnresolvedReference,
};

const ASTRO_BUILTIN_COMPONENTS: &[&str] = &["Fragment", "Code", "Debug"];

static SCRIPT_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?s)<script(\s[^>]*)?>(.*?)</script>").unwrap());
static BLOCK_OPEN_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<(script|style)(\s[^>]*)?>").unwrap());
static TEMPLATE_EXPR_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\{([^}/][^}]*)\}").unwrap());
static OPEN_EXPR_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\{([^}/][^}]*)$").unwrap());
static CALL_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b([a-zA-Z_$][A-Za-z0-9_$.]*)\s*\(").unwrap());
static COMPONENT_TAG_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<([A-Z][a-zA-Z0-9_$]*)\b").unwrap());

#[derive(Debug, Clone)]
struct SourceBlock {
    content: String,
    start_line: usize,
    end_line: Option<usize>,
}

pub struct AstroExtractor {
    file_path: String,
    source: String,
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    unresolved_references: Vec<UnresolvedReference>,
    errors: Vec<ExtractionError>,
}

impl AstroExtractor {
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

    /// Extract from Astro source.
    pub fn extract(mut self) -> ExtractionResult {
        let start = Instant::now();

        // `.astro` 文件本身作为 component 节点承载模板引用；嵌入脚本产出的
        // 函数/变量节点再通过 contains 挂到这个组件下。
        let component_node = self.create_component_node();
        let frontmatter = self.extract_frontmatter();
        if let Some(block) = &frontmatter {
            self.process_script_content(block, &component_node.id, "frontmatter");
        }
        for block in self.extract_script_blocks() {
            self.process_script_content(&block, &component_node.id, "script");
        }
        let covered_ranges = self.get_covered_ranges(frontmatter.as_ref());
        self.extract_template_calls(&component_node.id, &covered_ranges);
        self.extract_template_components(&component_node.id, &covered_ranges);

        ExtractionResult {
            nodes: self.nodes,
            edges: self.edges,
            unresolved_references: self.unresolved_references,
            errors: self.errors,
            duration_ms: start.elapsed().as_millis() as u64,
        }
    }

    /// Create a component node for the `.astro` file.
    fn create_component_node(&mut self) -> Node {
        let lines = self.source.split('\n').collect::<Vec<_>>();
        let file_name = basename(&self.file_path);
        let component_name = file_name
            .strip_suffix(".astro")
            .unwrap_or(&file_name)
            .to_owned();
        let id = generate_node_id(&self.file_path, NodeKind::Component, &component_name, 1);
        let node = Node {
            id,
            kind: NodeKind::Component,
            name: component_name.clone(),
            qualified_name: format!("{}::{}", self.file_path, component_name),
            file_path: self.file_path.clone(),
            language: Language::Astro,
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

    /// Extract the `---` frontmatter block, or `None` for absent/unclosed fences.
    fn extract_frontmatter(&self) -> Option<SourceBlock> {
        // 只接受文件开头第一个非空行的 `---` fence；中间出现的 `---`
        // 可能只是模板文本，不能当成 frontmatter。
        let lines = self.source.split('\n').collect::<Vec<_>>();
        let mut open_idx = None;
        for (idx, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if trimmed == "---" {
                open_idx = Some(idx);
            }
            break;
        }
        let open_idx = open_idx?;

        let mut close_idx = None;
        for (idx, line) in lines.iter().enumerate().skip(open_idx + 1) {
            if line.trim() == "---" {
                close_idx = Some(idx);
                break;
            }
        }
        let close_idx = close_idx?;

        Some(SourceBlock {
            content: lines[open_idx + 1..close_idx].join("\n"),
            start_line: open_idx + 1,
            end_line: Some(close_idx),
        })
    }

    /// Extract `<script>` blocks from the template portion.
    fn extract_script_blocks(&self) -> Vec<SourceBlock> {
        SCRIPT_REGEX
            .captures_iter(&self.source)
            .filter_map(|caps| {
                let full = caps.get(0)?;
                let content = caps.get(2).map(|m| m.as_str()).unwrap_or("").to_owned();
                let before_script = &self.source[..full.start()];
                let script_tag_line = before_script.matches('\n').count();
                let opening_tag = full
                    .as_str()
                    .split_once('>')
                    .map(|(tag, _)| format!("{tag}>"))
                    .unwrap_or_default();
                let opening_tag_lines = opening_tag.matches('\n').count();
                Some(SourceBlock {
                    content,
                    start_line: script_tag_line + opening_tag_lines,
                    end_line: None,
                })
            })
            .collect()
    }

    /// Process frontmatter / script content by delegating to TypeScript.
    fn process_script_content(
        &mut self,
        block: &SourceBlock,
        component_node_id: &str,
        label: &str,
    ) {
        if !is_language_supported(Language::TypeScript) {
            self.errors.push(ExtractionError {
                message: format!(
                    "Parser for typescript not available, cannot parse Astro {label} block"
                ),
                file_path: None,
                line: None,
                column: None,
                severity: ExtractionSeverity::Warning,
                code: None,
            });
            return;
        }

        let result = extract_from_source(
            &self.file_path,
            &block.content,
            Some(Language::TypeScript),
            None,
        );
        self.merge_embedded_result(result, component_node_id, block.start_line, Language::Astro);
    }

    fn merge_embedded_result(
        &mut self,
        result: ExtractionResult,
        component_node_id: &str,
        line_offset: usize,
        language: Language,
    ) {
        // 嵌入抽取器看到的是独立片段；合并回 Astro 时必须统一语言和行号，
        // 否则 MCP 展示的定位会跳到片段内而不是原始文件。
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

    /// Line ranges (0-indexed, inclusive) the template scans must skip.
    fn get_covered_ranges(&self, frontmatter: Option<&SourceBlock>) -> Vec<(usize, usize)> {
        // 模板扫描是 regex 级别的轻量补充；先排除 frontmatter、script、style，
        // 避免把脚本里的 JSX/调用重复记录成模板引用。
        let mut covered_ranges = Vec::new();
        if let Some(frontmatter) = frontmatter
            && let Some(end_line) = frontmatter.end_line
        {
            covered_ranges.push((frontmatter.start_line.saturating_sub(1), end_line));
        }
        covered_ranges.extend(covered_tag_ranges(&self.source));
        covered_ranges
    }

    /// Extract function calls from Astro template expressions.
    fn extract_template_calls(
        &mut self,
        component_node_id: &str,
        covered_ranges: &[(usize, usize)],
    ) {
        let lines = self.source.split('\n').collect::<Vec<_>>();

        for (line_idx, line) in lines.iter().enumerate() {
            if covered_ranges
                .iter()
                .any(|(start, end)| line_idx >= *start && line_idx <= *end)
            {
                continue;
            }

            let mut exprs = TEMPLATE_EXPR_REGEX
                .captures_iter(line)
                .filter_map(|caps| {
                    let full = caps.get(0)?;
                    let text = caps.get(1)?.as_str().to_owned();
                    Some((text, full.start()))
                })
                .collect::<Vec<_>>();

            let cleaned = TEMPLATE_EXPR_REGEX.replace_all(line, "");
            if let Some(open_caps) = OPEN_EXPR_REGEX.captures(&cleaned)
                && let Some(text) = open_caps.get(1).map(|m| m.as_str().to_owned())
            {
                // 宽容处理单行未闭合表达式，保留可能的调用线索；真正语法错误
                // 仍由上游 Astro/TS 解析器决定是否报告。
                exprs.push((text, line.rfind('{').unwrap_or(0)));
            }

            for (expr_text, expr_offset) in exprs {
                for call_match in CALL_REGEX.captures_iter(&expr_text) {
                    let Some(call_full) = call_match.get(0) else {
                        continue;
                    };
                    let Some(callee_name) = call_match.get(1).map(|m| m.as_str()) else {
                        continue;
                    };
                    if matches!(callee_name, "if" | "await" | "function") {
                        continue;
                    }
                    self.unresolved_references.push(UnresolvedReference {
                        from_node_id: component_node_id.to_owned(),
                        reference_name: callee_name.to_owned(),
                        reference_kind: ReferenceKind::Calls,
                        line: (line_idx + 1) as u64,
                        column: (expr_offset + call_full.start()) as u64,
                        file_path: Some(self.file_path.clone()),
                        language: Some(Language::Astro),
                        candidates: None,
                    });
                }
            }
        }
    }

    /// Extract component usages from the Astro template.
    fn extract_template_components(
        &mut self,
        component_node_id: &str,
        covered_ranges: &[(usize, usize)],
    ) {
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
                if ASTRO_BUILTIN_COMPONENTS.contains(&component_name) {
                    continue;
                }
                self.unresolved_references.push(UnresolvedReference {
                    from_node_id: component_node_id.to_owned(),
                    reference_name: component_name.to_owned(),
                    reference_kind: ReferenceKind::References,
                    line: (line_idx + 1) as u64,
                    column: (full.start() + 1) as u64,
                    file_path: Some(self.file_path.clone()),
                    language: Some(Language::Astro),
                    candidates: None,
                });
            }
        }
    }
}

fn covered_tag_ranges(source: &str) -> Vec<(usize, usize)> {
    // 这里不做完整 HTML 解析，只找到 script/style 的成对标签范围。
    // 未闭合标签按 opening tag 结束，避免把后续整个模板都屏蔽掉。
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
