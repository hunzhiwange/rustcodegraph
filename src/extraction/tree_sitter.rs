//! Tree-sitter parser wrapper.
//!
//! This is a structural Rust translation of `tree-sitter.ts`. Task 04 keeps
//! the core extraction phases and hook boundaries visible; Task 06 wires the
//! custom component/template extractors into the same special-case entry points.
//!
//! 这个模块是 AST 到 RustCodeGraph 基础图的主入口。它负责调度语言适配器、
//! 维护包含关系栈、记录未解析引用，并把少量跨语言启发式延迟到遍历结束后
//! 统一 flush，供 resolver 再解析成真实边。

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::Instant;

use crate::extraction::astro_extractor::AstroExtractor;
use crate::extraction::dfm_extractor::DfmExtractor;
use crate::extraction::function_ref::{
    FN_REF_SPECS, FnRefCandidate, FnRefSpec, capture_fn_ref_candidates,
};
use crate::extraction::generated_detection::is_generated_file;
use crate::extraction::grammars::{
    detect_language, get_parser, is_file_level_only_language, is_language_supported, language_key,
};
use crate::extraction::languages::index::extractor_for;
use crate::extraction::liquid_extractor::LiquidExtractor;
use crate::extraction::mybatis_extractor::MyBatisExtractor;
use crate::extraction::razor_extractor::RazorExtractor;
use crate::extraction::svelte_extractor::SvelteExtractor;
use crate::extraction::tree_sitter_helpers::{
    generate_node_id, get_child_by_field, get_node_text, get_preceding_docstring,
};
use crate::extraction::tree_sitter_types::{
    ClassNodeKind, ExtractorContext, LanguageExtractor, MethodNodeKind, NodeExtra,
    UnresolvedReferenceInput,
};
use crate::extraction::vue_extractor::VueExtractor;
use crate::types::{
    Edge, EdgeKind, EdgeProvenance, ExtractionError, ExtractionResult, ExtractionSeverity,
    Language, Node, NodeKind, ReferenceKind, UnresolvedReference, Visibility,
};
use crate::web_tree_sitter::{SyntaxNode, Tree};

mod calls;
mod context;
mod core;
mod declarations;
mod decorators;
mod fallback;
mod imports;
mod misc;
mod objc;
mod pascal;
mod pascal_calls;
mod public_api;
mod source_scan;
mod ts_utils;
mod type_refs;
mod value_refs;
mod variables;

use context::CoreExtractorContext;
use decorators::*;
use fallback::*;
use misc::*;
use objc::*;
pub use public_api::extract_from_source;
use public_api::{
    count_value_declarations_in_source, edge, extraction_error, extraction_result,
    extraction_result_now, fn_ref_spec_for_language, java_decl_names_from_text,
    language_extractor_for, node_from_parts, node_id, node_kind, node_name, unresolved_reference,
    value_ref_decl_is_target_scope,
};
use source_scan::*;
use ts_utils::*;

const VALUE_REF_LANGS: &[&str] = &[
    "typescript",
    "javascript",
    "tsx",
    "go",
    "python",
    "rust",
    "ruby",
    "c",
    "java",
    "csharp",
    "php",
    "scala",
    "kotlin",
    "swift",
    "dart",
    "pascal",
];

// value-ref 是为了补足常量/配置名的轻量数据流；节点过多时宁可少连边，
// 也不要让一次索引在巨型文件上退化成全 AST 多轮扫描。
const MAX_VALUE_REF_NODES: usize = 20_000;
#[allow(dead_code)]
const PHP_TYPE_NODES: &[&str] = &[
    "named_type",
    "optional_type",
    "nullable_type",
    "union_type",
    "intersection_type",
    "disjunctive_normal_form_type",
    "primitive_type",
];
const MEMBER_ACCESS_TYPES: &[&str] = &[
    "field_access",
    "member_access_expression",
    "navigation_expression",
    "field_expression",
    "class_constant_access_expression",
    "scoped_property_access_expression",
    "qualified_identifier",
];
// 只有这些语言把 `Type.member` 形态稳定表示为静态类型引用；动态语言若照搬
// 会把普通对象访问误连到类型节点。
const STATIC_MEMBER_LANGS: &[&str] = &[
    "java", "csharp", "kotlin", "swift", "scala", "dart", "php", "cpp",
];
const INSTANTIATION_KINDS: &[&str] = &[
    "new_expression",
    "object_creation_expression",
    "instance_creation_expression",
    "composite_literal",
    "struct_expression",
    "instance_expression",
];

#[derive(Debug, Clone)]
struct FnRefCandidateInScope {
    // 函数引用候选先按语法捕获，等所有节点创建完后再判断是否值得发引用。
    candidate: FnRefCandidate,
    from_node_id: String,
}

#[derive(Debug, Clone)]
struct ValueRefScope {
    // 保存能承载 value-ref 边的局部子树；flush 阶段才知道全文件声明是否唯一。
    id: String,
    node: SyntaxNode,
    name: String,
    is_value_binding: bool,
    is_target_scope: bool,
}

/// Extract the name from a node using the adapter hook, configured name field,
/// and the same fallback sequence as the TypeScript extractor.
fn extract_name(node: &SyntaxNode, source: &str, extractor: &dyn LanguageExtractor) -> String {
    if let Some(hook_name) = extractor.resolve_name(node, source)
        && !hook_name.is_empty()
    {
        return hook_name;
    }

    if let Some(name_node) = get_child_by_field(node, extractor.name_field()) {
        let mut resolved = name_node;
        while resolved.node_type() == "pointer_declarator" {
            let Some(inner) =
                get_child_by_field(resolved, "declarator").or_else(|| resolved.named_child(0))
            else {
                break;
            };
            resolved = inner;
        }

        if matches!(resolved.node_type(), "function_declarator" | "declarator") {
            let inner_name =
                get_child_by_field(resolved, "declarator").or_else(|| resolved.named_child(0));
            return inner_name
                .map(|inner| get_node_text(inner, source))
                .unwrap_or_else(|| get_node_text(resolved, source));
        }

        if resolved.node_type() == "dot_index_expression"
            && let Some(field) = get_child_by_field(resolved, "field")
        {
            return get_node_text(field, source);
        }
        if resolved.node_type() == "method_index_expression"
            && let Some(method) = get_child_by_field(resolved, "method")
        {
            return get_node_text(method, source);
        }
        return get_node_text(resolved, source);
    }

    if node.node_type() == "method_signature" {
        for child in &node.named_children {
            if matches!(
                child.node_type(),
                "function_signature"
                    | "getter_signature"
                    | "setter_signature"
                    | "constructor_signature"
                    | "factory_constructor_signature"
            ) && let Some(inner) = child
                .named_children
                .iter()
                .find(|inner| inner.node_type() == "identifier")
            {
                return get_node_text(inner, source);
            }
        }
    }

    if matches!(node.node_type(), "arrow_function" | "function_expression") {
        return "<anonymous>".to_owned();
    }

    for child in &node.named_children {
        if matches!(
            child.node_type(),
            "identifier" | "type_identifier" | "simple_identifier" | "constant"
        ) {
            return get_node_text(child, source);
        }
    }

    "<anonymous>".to_owned()
}

#[allow(dead_code)]
fn scala_base_type_name(node: Option<&SyntaxNode>, source: &str) -> Option<String> {
    let node = node?;
    match node.node_type() {
        "type_identifier" | "identifier" => Some(get_node_text(node, source)),
        "generic_type" => scala_base_type_name(node.named_child(0), source),
        "stable_type_identifier" | "stable_identifier" => node
            .named_children
            .iter()
            .rfind(|child| matches!(child.node_type(), "type_identifier" | "identifier"))
            .map(|child| get_node_text(child, source)),
        _ => node
            .named_children
            .iter()
            .find(|child| child.node_type() == "type_identifier")
            .map(|child| get_node_text(child, source)),
    }
}

#[allow(dead_code)]
fn c_declarator_identifier(node: Option<&SyntaxNode>) -> Option<&SyntaxNode> {
    let mut cur = node;
    for _ in 0..12 {
        let current = cur?;
        match current.node_type() {
            "identifier" => return Some(current),
            "function_declarator" => return None,
            "init_declarator"
            | "pointer_declarator"
            | "array_declarator"
            | "parenthesized_declarator" => {
                cur = get_child_by_field(current, "declarator");
            }
            _ => return None,
        }
    }
    None
}

#[allow(dead_code)]
fn first_simple_identifier(node: Option<&SyntaxNode>) -> Option<SyntaxNode> {
    let mut queue = node.cloned().into_iter().collect::<Vec<_>>();
    let mut guard = 0;
    while let Some(current) = queue.first().cloned() {
        queue.remove(0);
        guard += 1;
        if guard > 40 {
            return None;
        }
        if current.node_type() == "simple_identifier" {
            return Some(current);
        }
        queue.extend(current.named_children.iter().cloned());
    }
    None
}

#[allow(dead_code)]
fn has_function_ancestor(node: &SyntaxNode) -> bool {
    let mut parent = node.parent.as_deref();
    while let Some(current) = parent {
        if current.node_type() == "function_definition" {
            return true;
        }
        parent = current.parent.as_deref();
    }
    false
}

pub struct TreeSitterExtractor {
    file_path: String,
    language: Language,
    source: String,
    tree: Option<Tree>,
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    unresolved_references: Vec<UnresolvedReference>,
    value_refs_enabled: bool,
    file_scope_values: HashMap<String, String>,
    file_scope_value_counts: HashMap<String, usize>,
    value_ref_scopes: Vec<ValueRefScope>,
    errors: Vec<ExtractionError>,
    extractor: Option<&'static dyn LanguageExtractor>,
    node_stack: Vec<String>,
    method_index: Option<HashMap<String, String>>,
    pascal_visibility_stack: Vec<Visibility>,
    fn_ref_spec: Option<FnRefSpec>,
    fn_ref_candidates: Vec<FnRefCandidateInScope>,
}

impl TreeSitterExtractor {
    /// 构造抽取器时只做语言探测和静态配置绑定，真正的 parser 获取和 AST 遍历
    /// 延迟到 `extract`，方便上层统一处理特殊文件和错误结果。
    pub fn new(
        file_path: impl Into<String>,
        source: impl Into<String>,
        language: Option<Language>,
    ) -> Self {
        let file_path = file_path.into();
        let source = source.into();
        let language = language.unwrap_or_else(|| detect_language(&file_path, Some(&source)));
        let fn_ref_spec = fn_ref_spec_for_language(&language);
        Self {
            file_path,
            language,
            source,
            tree: None,
            nodes: Vec::new(),
            edges: Vec::new(),
            unresolved_references: Vec::new(),
            value_refs_enabled: std::env::var("RUSTCODEGRAPH_VALUE_REFS").ok().as_deref()
                != Some("0"),
            file_scope_values: HashMap::new(),
            file_scope_value_counts: HashMap::new(),
            value_ref_scopes: Vec::new(),
            errors: Vec::new(),
            extractor: language_extractor_for(&language),
            node_stack: Vec::new(),
            method_index: None,
            pascal_visibility_stack: Vec::new(),
            fn_ref_spec,
            fn_ref_candidates: Vec::new(),
        }
    }

    pub fn extract(mut self) -> ExtractionResult {
        let start = Instant::now();

        // 不支持的语言返回带错误的结果，调用方可以继续索引其它文件。
        if !is_language_supported(self.language) {
            return extraction_result(
                Vec::new(),
                Vec::new(),
                Vec::new(),
                vec![extraction_error(
                    format!("Unsupported language: {}", language_key(&self.language)),
                    Some(self.file_path.clone()),
                    ExtractionSeverity::Error,
                    Some("unsupported_language"),
                )],
                start,
            );
        }

        // 纯文本/配置类文件只在更高层记录 file 级信息，这里不创建符号节点。
        if is_file_level_only_language(self.language) {
            return extraction_result(Vec::new(), Vec::new(), Vec::new(), Vec::new(), start);
        }

        let Some(mut parser) = get_parser(self.language) else {
            return extraction_result(
                Vec::new(),
                Vec::new(),
                Vec::new(),
                vec![extraction_error(
                    format!(
                        "Failed to get parser for language: {}",
                        language_key(&self.language)
                    ),
                    Some(self.file_path.clone()),
                    ExtractionSeverity::Error,
                    Some("parser_error"),
                )],
                start,
            );
        };

        // 某些 grammar 需要在解析前做轻量修正；语言适配器必须保持行号稳定。
        if let Some(extractor) = self.extractor
            && let Some(transformed) = extractor.pre_parse(&self.source)
        {
            self.source = transformed;
        }

        let Some(tree) = parser.parse(&self.source, None) else {
            return extraction_result(
                Vec::new(),
                Vec::new(),
                Vec::new(),
                vec![extraction_error(
                    "Parser returned null tree".to_owned(),
                    Some(self.file_path.clone()),
                    ExtractionSeverity::Error,
                    Some("parser_error"),
                )],
                start,
            );
        };

        // file 节点始终位于 node_stack 底部，后续 create_node 会自动补 contains。
        if let Some(file_node) = self.create_file_node() {
            self.node_stack.push(node_id(&file_node));
            self.nodes.push(file_node);
        }

        if let Some(package_id) = self.extract_file_package(&tree.root_node) {
            self.node_stack.push(package_id);
        }

        self.visit_node(&tree.root_node);
        // 函数引用和值引用依赖“全文件节点已经建完”这个前提，遍历后统一发边。
        self.flush_fn_ref_candidates();
        self.tree = Some(tree);
        self.flush_value_refs();

        // 大源码和 AST 不再需要，提前释放，避免批量索引时内存峰值随文件数累积。
        self.node_stack.clear();
        self.tree = None;
        self.source.clear();

        extraction_result(
            self.nodes,
            self.edges,
            self.unresolved_references,
            self.errors,
            start,
        )
    }
}
