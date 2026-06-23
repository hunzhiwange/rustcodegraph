//! Rust translation of `src/extraction/razor-extractor.ts`.
//!
//! Razor/Blazor markup names C# types and components from places the generic
//! extractor cannot see. This module preserves the TS regex passes and the
//! embedded C# delegation for `@code` / `@functions` / `@{...}` blocks.

use std::time::{Instant, SystemTime, UNIX_EPOCH};

use regex::Regex;
use std::sync::LazyLock;

use crate::extraction::grammars::is_language_supported;
use crate::extraction::tree_sitter::extract_from_source;
use crate::extraction::tree_sitter_helpers::generate_node_id;
use crate::types::{
    Edge, ExtractionError, ExtractionResult, Language, Node, NodeKind, ReferenceKind,
    UnresolvedReference,
};

const BLAZOR_BUILTIN_COMPONENTS: &[&str] = &[
    "Router",
    "Found",
    "NotFound",
    "RouteView",
    "AuthorizeRouteView",
    "LayoutView",
    "CascadingValue",
    "CascadingAuthenticationState",
    "AuthorizeView",
    "Authorized",
    "NotAuthorized",
    "Authorizing",
    "EditForm",
    "DataAnnotationsValidator",
    "ValidationSummary",
    "ValidationMessage",
    "InputText",
    "InputNumber",
    "InputCheckbox",
    "InputSelect",
    "InputDate",
    "InputTextArea",
    "InputRadio",
    "InputRadioGroup",
    "InputFile",
    "PageTitle",
    "HeadContent",
    "HeadOutlet",
    "Virtualize",
    "DynamicComponent",
    "ErrorBoundary",
    "SectionContent",
    "SectionOutlet",
    "FocusOnNavigate",
    "NavLink",
    "Microsoft",
];

static TYPE_SPLIT_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[<>,\s]+").unwrap());
static TYPE_NAME_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Z][A-Za-z0-9_]*$").unwrap());
static DIRECTIVE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*@(?:model|inherits)\s+([A-Za-z_][\w.]*(?:\s*<[^>]+>)?)").unwrap()
});
static INJECT_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*@inject\s+([A-Za-z_][\w.]*(?:\s*<[^>]+>)?)\s+[A-Za-z_]").unwrap()
});
static TYPEOF_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"@typeof\(\s*([A-Za-z_][\w.]*)\s*\)").unwrap());
static TAG_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<([A-Z][A-Za-z0-9_]*)\b([^>]*)>").unwrap());
static TYPE_ARG_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"\bT[A-Za-z]*\s*=\s*"([A-Za-z_][\w.]*)""#).unwrap());
static CODE_BLOCK_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"@(?:code|functions)\b\s*\{|@\{").unwrap());

#[derive(Debug, Clone)]
struct CodeBlock {
    content: String,
    line_offset: usize,
}

pub struct RazorExtractor {
    file_path: String,
    source: String,
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    unresolved_references: Vec<UnresolvedReference>,
    errors: Vec<ExtractionError>,
}

impl RazorExtractor {
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

    pub fn extract(mut self) -> ExtractionResult {
        let start = Instant::now();

        let component_id = self.create_component_node().id;
        self.extract_directives(&component_id);
        // `.cshtml` 也走 Razor 指令和代码块解析，但 Blazor 组件标签只在 `.razor` 中有意义。
        if self.file_path.to_ascii_lowercase().ends_with(".razor") {
            self.extract_component_tags(&component_id);
        }
        self.process_code_blocks(&component_id);

        ExtractionResult {
            nodes: self.nodes,
            edges: self.edges,
            unresolved_references: self.unresolved_references,
            errors: self.errors,
            duration_ms: start.elapsed().as_millis() as u64,
        }
    }

    fn create_component_node(&mut self) -> Node {
        let lines = self.source.split('\n').collect::<Vec<_>>();
        let file_name = basename(&self.file_path);
        let component_name = strip_razor_extension(&file_name);
        let id = generate_node_id(&self.file_path, NodeKind::Component, &component_name, 1);
        let node = Node {
            id,
            kind: NodeKind::Component,
            name: component_name.clone(),
            qualified_name: format!("{}::{}", self.file_path, component_name),
            file_path: self.file_path.clone(),
            language: Language::Razor,
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

    /// Last `.`-segment (`App.ViewModels.RegisterModel` -> `RegisterModel`).
    fn last_segment<'a>(&self, value: &'a str) -> &'a str {
        value.rsplit('.').next().unwrap_or(value)
    }

    /// Split a type expression into capitalized type names, including generics.
    fn type_names(&self, expr: &str) -> Vec<String> {
        TYPE_SPLIT_REGEX
            .split(expr)
            .filter_map(|raw| {
                let seg = self.last_segment(raw.trim());
                TYPE_NAME_REGEX.is_match(seg).then(|| seg.to_owned())
            })
            .collect()
    }

    fn push_ref(&mut self, component_id: &str, name: &str, line: usize, column: usize) {
        // 所有 Razor 外部类型/组件引用都归因到组件节点，避免为临时 markup 片段制造噪声节点。
        self.unresolved_references.push(UnresolvedReference {
            from_node_id: component_id.to_owned(),
            reference_name: name.to_owned(),
            reference_kind: ReferenceKind::References,
            line: line as u64,
            column: column as u64,
            file_path: Some(self.file_path.clone()),
            language: Some(Language::Razor),
            candidates: None,
        });
    }

    fn extract_directives(&mut self, component_id: &str) {
        let lines = self
            .source
            .split('\n')
            .map(str::to_owned)
            .collect::<Vec<_>>();
        for (idx, line) in lines.iter().enumerate() {
            if let Some(caps) = DIRECTIVE_REGEX.captures(line)
                && let Some(expr) = caps.get(1).map(|m| m.as_str())
            {
                for type_name in self.type_names(expr) {
                    self.push_ref(component_id, &type_name, idx + 1, 0);
                }
            }
            if let Some(caps) = INJECT_REGEX.captures(line)
                && let Some(expr) = caps.get(1).map(|m| m.as_str())
            {
                for type_name in self.type_names(expr) {
                    self.push_ref(component_id, &type_name, idx + 1, 0);
                }
            }
            let typeof_refs = TYPEOF_REGEX
                .captures_iter(line)
                .filter_map(|caps| {
                    let full = caps.get(0)?;
                    let raw = caps.get(1)?.as_str();
                    let seg = self.last_segment(raw);
                    seg.chars()
                        .next()
                        .is_some_and(|ch| ch.is_ascii_uppercase())
                        .then(|| (seg.to_owned(), full.start()))
                })
                .collect::<Vec<_>>();
            for (seg, column) in typeof_refs {
                // markup 中的 @typeof(...) 不会进入 C# code block，需要在指令扫描阶段补引用。
                self.push_ref(component_id, &seg, idx + 1, column);
            }
        }
    }

    fn extract_component_tags(&mut self, component_id: &str) {
        let lines = self
            .source
            .split('\n')
            .map(str::to_owned)
            .collect::<Vec<_>>();
        for (idx, line) in lines.iter().enumerate() {
            let tag_matches = TAG_REGEX
                .captures_iter(line)
                .filter_map(|caps| {
                    let full = caps.get(0)?;
                    let name = caps.get(1)?.as_str().to_owned();
                    let attrs = caps.get(2).map(|m| m.as_str()).unwrap_or("").to_owned();
                    Some((full.start(), name, attrs))
                })
                .collect::<Vec<_>>();
            for (start, name, attrs) in tag_matches {
                if BLAZOR_BUILTIN_COMPONENTS.contains(&name.as_str()) {
                    continue;
                }
                // 首字母大写 tag 视为组件引用，内置组件白名单过滤掉框架自带节点。
                self.push_ref(component_id, &name, idx + 1, start + 1);

                let type_args = TYPE_ARG_REGEX
                    .captures_iter(&attrs)
                    .filter_map(|caps| {
                        let raw = caps.get(1)?.as_str();
                        let seg = self.last_segment(raw);
                        seg.chars()
                            .next()
                            .is_some_and(|ch| ch.is_ascii_uppercase())
                            .then(|| seg.to_owned())
                    })
                    .collect::<Vec<_>>();
                for seg in type_args {
                    self.push_ref(component_id, &seg, idx + 1, 0);
                }
            }
        }
    }

    /// Find the matching `}` for the `{` at `open_idx`, skipping strings/comments.
    fn match_brace(&self, open_idx: usize) -> Option<usize> {
        let bytes = self.source.as_bytes();
        let mut depth = 0isize;
        let mut idx = open_idx;

        // 简单状态机跳过字符串和注释，防止代码块内部的 `}` 误结束 Razor block。
        while idx < bytes.len() {
            match bytes[idx] {
                b'"' | b'\'' => {
                    let quote = bytes[idx];
                    idx += 1;
                    while idx < bytes.len() && bytes[idx] != quote {
                        if bytes[idx] == b'\\' {
                            idx += 1;
                        }
                        idx += 1;
                    }
                    idx += 1;
                    continue;
                }
                b'/' if bytes.get(idx + 1) == Some(&b'/') => {
                    while idx < bytes.len() && bytes[idx] != b'\n' {
                        idx += 1;
                    }
                    continue;
                }
                b'/' if bytes.get(idx + 1) == Some(&b'*') => {
                    idx += 2;
                    while idx + 1 < bytes.len() && !(bytes[idx] == b'*' && bytes[idx + 1] == b'/') {
                        idx += 1;
                    }
                    idx = (idx + 2).min(bytes.len());
                    continue;
                }
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(idx);
                    }
                }
                _ => {}
            }
            idx += 1;
        }
        None
    }

    /// `@code { ... }` / `@functions { ... }` and `@{ ... }` C# blocks.
    fn extract_code_blocks(&self) -> Vec<CodeBlock> {
        let mut blocks = Vec::new();
        let mut search_start = 0usize;

        while let Some(caps) = CODE_BLOCK_REGEX.captures(&self.source[search_start..]) {
            let Some(full) = caps.get(0) else {
                break;
            };
            let match_start = search_start + full.start();
            let Some(open_rel) = self.source[match_start..].find('{') else {
                search_start = match_start + 1;
                continue;
            };
            let open_idx = match_start + open_rel;
            let Some(close_idx) = self.match_brace(open_idx) else {
                search_start = open_idx + 1;
                continue;
            };
            let content = self.source[open_idx + 1..close_idx].to_owned();
            let line_offset = self.source[..open_idx + 1].matches('\n').count();
            blocks.push(CodeBlock {
                content,
                line_offset,
            });
            search_start = close_idx;
        }

        blocks
    }

    /// Delegate C# code blocks and attribute external references to the component.
    fn process_code_blocks(&mut self, component_id: &str) {
        if !is_language_supported(Language::CSharp) {
            return;
        }
        for block in self.extract_code_blocks() {
            if block.content.trim().is_empty() {
                continue;
            }
            let mut seen_type_refs = Vec::new();
            for type_name in self.type_names(&block.content) {
                if !seen_type_refs.contains(&type_name) {
                    self.push_ref(component_id, &type_name, block.line_offset + 1, 0);
                    seen_type_refs.push(type_name);
                }
            }
            let wrapped = format!("class __RazorCode__ {{\n{}\n}}", block.content);
            // 把片段包进临时类交给 C# 抽取器，返回的引用再平移回 Razor 原始行号。
            let result =
                extract_from_source(&self.file_path, &wrapped, Some(Language::CSharp), None);
            for reference in result.unresolved_references {
                self.unresolved_references.push(UnresolvedReference {
                    from_node_id: component_id.to_owned(),
                    reference_name: reference.reference_name,
                    reference_kind: reference.reference_kind,
                    line: reference
                        .line
                        .saturating_add(block.line_offset as u64)
                        .saturating_sub(1),
                    column: reference.column,
                    file_path: Some(self.file_path.clone()),
                    language: Some(Language::Razor),
                    candidates: reference.candidates,
                });
            }
        }
    }
}

fn strip_razor_extension(file_name: &str) -> String {
    let lower = file_name.to_ascii_lowercase();
    if lower.ends_with(".cshtml") {
        file_name[..file_name.len() - ".cshtml".len()].to_owned()
    } else if lower.ends_with(".razor") {
        file_name[..file_name.len() - ".razor".len()].to_owned()
    } else {
        file_name.to_owned()
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
