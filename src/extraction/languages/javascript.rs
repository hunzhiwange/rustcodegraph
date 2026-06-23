//! Rust translation of `src/extraction/languages/javascript.ts`.

use crate::extraction::languages::typescript::{
    classify_ts_class_member, extract_ts_js_variables, resolve_ts_field_body,
};
use crate::extraction::tree_sitter_helpers::{get_child_by_field, get_node_text};
use crate::extraction::tree_sitter_types::{
    ImportInfo, LanguageExtractor, MethodClassification, VariableInfo,
};
use crate::web_tree_sitter::SyntaxNode;

// JavaScript 与 TypeScript 的类字段、变量声明形态高度一致，复用 TS helper 能保持
// `.js/.jsx/.ts/.tsx` 对导出对象、字段函数和初始化器的解释一致。
pub struct JavascriptExtractor;

pub const JAVASCRIPT_EXTRACTOR: JavascriptExtractor = JavascriptExtractor;

pub fn javascript_extractor() -> &'static JavascriptExtractor {
    &JAVASCRIPT_EXTRACTOR
}

impl LanguageExtractor for JavascriptExtractor {
    fn function_types(&self) -> &'static [&'static str] {
        &[
            "function_declaration",
            "arrow_function",
            "function_expression",
        ]
    }

    fn class_types(&self) -> &'static [&'static str] {
        &["class_declaration"]
    }

    fn method_types(&self) -> &'static [&'static str] {
        &["method_definition", "field_definition"]
    }

    fn classify_method_node(&self, node: &SyntaxNode) -> MethodClassification {
        classify_ts_class_member(node)
    }

    fn interface_types(&self) -> &'static [&'static str] {
        &[]
    }

    fn struct_types(&self) -> &'static [&'static str] {
        &[]
    }

    fn enum_types(&self) -> &'static [&'static str] {
        &[]
    }

    fn type_alias_types(&self) -> &'static [&'static str] {
        &[]
    }

    fn import_types(&self) -> &'static [&'static str] {
        &["import_statement"]
    }

    fn call_types(&self) -> &'static [&'static str] {
        &["call_expression"]
    }

    fn variable_types(&self) -> &'static [&'static str] {
        &["lexical_declaration", "variable_declaration"]
    }

    fn name_field(&self) -> &'static str {
        "name"
    }

    fn resolve_name(&self, node: &SyntaxNode, source: &str) -> Option<String> {
        if node.type_name() == "field_definition" {
            // JS class field 的字段名在 property 字段，而不是通用 name 字段。
            return get_child_by_field(node, "property").map(|prop| get_node_text(prop, source));
        }
        None
    }

    fn body_field(&self) -> &'static str {
        "body"
    }

    fn resolve_body(&self, node: &SyntaxNode, body_field: &str) -> Option<SyntaxNode> {
        // `field = () => {}` 应作为方法处理时，body 在初始化函数里而不在字段节点上。
        resolve_ts_field_body(node, "field_definition", body_field)
            .or_else(|| node.child_for_field_name(body_field).cloned())
    }

    fn params_field(&self) -> &'static str {
        "parameters"
    }

    fn get_signature(&self, node: &SyntaxNode, source: &str) -> Option<String> {
        get_child_by_field(node, "parameters").map(|params| get_node_text(params, source))
    }

    fn is_exported(&self, node: &SyntaxNode, _source: &str) -> bool {
        let mut current = node.parent();
        while let Some(parent) = current {
            if parent.type_name() == "export_statement" {
                return true;
            }
            current = parent.parent();
        }
        false
    }

    fn is_async(&self, node: &SyntaxNode) -> bool {
        (0..node.child_count()).any(|i| node.child(i).is_some_and(|c| c.type_name() == "async"))
    }

    fn is_const(&self, node: &SyntaxNode) -> bool {
        node.type_name() == "lexical_declaration"
            && (0..node.child_count())
                .any(|i| node.child(i).is_some_and(|c| c.type_name() == "const"))
    }

    fn extract_import(&self, node: &SyntaxNode, source: &str) -> Option<ImportInfo> {
        let source_field = node.child_for_field_name("source")?;
        let module_name = get_node_text(source_field, source).replace(['\'', '"'], "");
        (!module_name.is_empty()).then(|| ImportInfo {
            module_name,
            signature: get_node_text(node, source).trim().to_string(),
            handled_refs: false,
        })
    }

    fn extract_variables(&self, node: &SyntaxNode, source: &str) -> Vec<VariableInfo> {
        // 变量委托给 TS/JS 共用逻辑，特别是 `const fn = () =>` 和导出对象中的函数。
        extract_ts_js_variables(node, source, self.is_const(node))
    }
}
