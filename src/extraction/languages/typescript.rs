//! Rust translation of `src/extraction/languages/typescript.ts`.

use crate::extraction::tree_sitter_helpers::{get_child_by_field, get_node_text};
use crate::extraction::tree_sitter_types::{
    ImportInfo, LanguageExtractor, MethodClassification, VariableInfo, Visibility,
};
use crate::types::NodeKind;
use crate::web_tree_sitter::SyntaxNode;

pub struct TypescriptExtractor;

pub const TYPESCRIPT_EXTRACTOR: TypescriptExtractor = TypescriptExtractor;

pub fn typescript_extractor() -> &'static TypescriptExtractor {
    &TYPESCRIPT_EXTRACTOR
}

/// A TS/JS class field is a method only when its value is callable.
pub fn classify_ts_class_member(node: &SyntaxNode) -> MethodClassification {
    if node.type_name() != "public_field_definition" && node.type_name() != "field_definition" {
        return MethodClassification::Method;
    }

    for i in 0..node.named_child_count() {
        let Some(child) = node.named_child(i) else {
            continue;
        };
        if child.type_name() == "arrow_function" || child.type_name() == "function_expression" {
            return MethodClassification::Method;
        }
        if child.type_name() == "call_expression" {
            // 装饰器/工厂式字段里常把函数作为参数传入，也应按方法处理以保留 body。
            let Some(args) = get_child_by_field(child, "arguments") else {
                continue;
            };
            for j in 0..args.named_child_count() {
                let Some(arg) = args.named_child(j) else {
                    continue;
                };
                if arg.type_name() == "arrow_function" || arg.type_name() == "function_expression" {
                    return MethodClassification::Method;
                }
            }
        }
    }

    MethodClassification::Property
}

fn field_function_body(
    node: &SyntaxNode,
    field_node_type: &str,
    body_field: &str,
) -> Option<SyntaxNode> {
    if node.type_name() != field_node_type {
        return None;
    }

    for i in 0..node.named_child_count() {
        let child = node.named_child(i)?.clone();
        if child.type_name() == "arrow_function" || child.type_name() == "function_expression" {
            return get_child_by_field(&child, body_field).cloned();
        }
        if child.type_name() == "call_expression" {
            // `field = factory(() => {...})` 的执行体在调用参数中。
            let Some(args) = get_child_by_field(&child, "arguments") else {
                continue;
            };
            for j in 0..args.named_child_count() {
                let Some(arg) = args.named_child(j) else {
                    continue;
                };
                if arg.type_name() == "arrow_function" || arg.type_name() == "function_expression" {
                    return get_child_by_field(arg, body_field).cloned();
                }
            }
        }
    }

    None
}

fn is_function_value(node: &SyntaxNode) -> bool {
    matches!(node.node_type(), "arrow_function" | "function_expression")
}

fn is_object_literal(node: &SyntaxNode) -> bool {
    matches!(node.node_type(), "object" | "object_expression")
}

fn is_exported_ts_js_declaration(node: &SyntaxNode) -> bool {
    let mut current = node.parent();
    while let Some(parent) = current {
        if parent.type_name() == "export_statement" {
            return true;
        }
        current = parent.parent();
    }
    false
}

fn object_from_parenthesized(node: &SyntaxNode) -> Option<SyntaxNode> {
    if is_object_literal(node) {
        return Some(node.clone());
    }
    if node.node_type() != "parenthesized_expression" {
        return None;
    }
    node.named_children
        .iter()
        .find_map(object_from_parenthesized)
}

fn function_returned_object(function: &SyntaxNode) -> Option<SyntaxNode> {
    let body = get_child_by_field(function, "body")?;
    if let Some(object) = object_from_parenthesized(body) {
        return Some(object);
    }
    if body.node_type() != "statement_block" {
        return None;
    }
    body.named_children
        .iter()
        .filter(|child| child.node_type() == "return_statement")
        .find_map(|statement| {
            statement
                .named_children
                .iter()
                .find_map(object_from_parenthesized)
        })
}

fn find_initializer_returned_object(call_node: &SyntaxNode, depth: usize) -> Option<SyntaxNode> {
    if depth > 4 {
        // 初始化器链只做浅层追踪，避免在高度动态工厂调用里展开过深。
        return None;
    }
    let args = get_child_by_field(call_node, "arguments")?;
    for arg in &args.named_children {
        if is_function_value(arg) {
            if let Some(object) = function_returned_object(arg) {
                return Some(object);
            }
        } else if arg.node_type() == "call_expression"
            && let Some(object) = find_initializer_returned_object(arg, depth + 1)
        {
            return Some(object);
        }
    }
    None
}

fn initializer_signature(value: &SyntaxNode, source: &str) -> Option<String> {
    let text = get_node_text(value, source);
    if text.is_empty() {
        return None;
    }

    let mut chars = text.chars();
    let preview = chars.by_ref().take(100).collect::<String>();
    let suffix = if chars.next().is_some() { "..." } else { "" };
    Some(format!("= {preview}{suffix}"))
}

fn field_signature(node: &SyntaxNode, source: &str) -> Option<String> {
    if node.type_name() != "public_field_definition" && node.type_name() != "field_definition" {
        return None;
    }
    let name = get_child_by_field(node, "name")
        .map(|name| get_node_text(name, source))
        .filter(|name| !name.is_empty())?;
    let text = get_node_text(node, source);
    let text = text.trim().trim_end_matches(';').trim();
    let name_start = text.find(&name)?;
    let after_name = text[name_start + name.len()..].trim_start();
    let eq_index = after_name.find('=');
    let colon_index = after_name
        .find(':')
        .filter(|colon| eq_index.map(|eq| *colon < eq).unwrap_or(true))?;
    let end = eq_index.unwrap_or(after_name.len());
    let type_text = after_name[colon_index + 1..end].trim();
    (!type_text.is_empty()).then(|| format!("{type_text} {name}"))
}

pub(crate) fn extract_ts_js_variables(
    node: &SyntaxNode,
    source: &str,
    is_const: bool,
) -> Vec<VariableInfo> {
    let kind = if is_const {
        NodeKind::Constant
    } else {
        NodeKind::Variable
    };
    let is_exported = is_exported_ts_js_declaration(node);
    let mut variables = Vec::new();

    for child in &node.named_children {
        if child.node_type() != "variable_declarator" {
            continue;
        }

        let Some(name_node) = get_child_by_field(child, "name") else {
            continue;
        };
        if matches!(name_node.node_type(), "object_pattern" | "array_pattern") {
            continue;
        }

        let name = get_node_text(name_node, source);
        if name.is_empty() {
            continue;
        }

        let value_node = get_child_by_field(child, "value").cloned();
        if let Some(value) = value_node.as_ref().filter(|value| is_function_value(value)) {
            // `const fn = () => {}` 在图上更像 function；delegate_to_function 让通用 visitor 访问真实 body。
            variables.push(VariableInfo {
                name,
                kind: NodeKind::Function,
                signature: None,
                is_exported: Some(is_exported),
                delegate_to_function: Some(value.clone()),
                position_node: Some(child.clone()),
                visit_value: None,
                object_literal_functions: None,
            });
            continue;
        }

        let object_literal_functions = if is_exported {
            // 导出的对象字面量经常承载 store/actions；只对导出值挖内部函数，控制图规模。
            value_node.as_ref().and_then(|value| {
                if is_object_literal(value) {
                    Some(value.clone())
                } else if value.node_type() == "call_expression" {
                    find_initializer_returned_object(value, 0)
                } else {
                    None
                }
            })
        } else {
            None
        };
        let visit_value = value_node
            .as_ref()
            .filter(|value| !is_object_literal(value))
            .filter(|value| {
                !(object_literal_functions.is_some() && value.node_type() == "call_expression")
            })
            .cloned();
        variables.push(VariableInfo {
            name,
            kind,
            signature: value_node
                .as_ref()
                .and_then(|value| initializer_signature(value, source)),
            is_exported: Some(is_exported),
            delegate_to_function: None,
            position_node: Some(child.clone()),
            visit_value,
            object_literal_functions,
        });
    }

    variables
}

impl LanguageExtractor for TypescriptExtractor {
    fn function_types(&self) -> &'static [&'static str] {
        &[
            "function_declaration",
            "arrow_function",
            "function_expression",
        ]
    }

    fn class_types(&self) -> &'static [&'static str] {
        &["class_declaration", "abstract_class_declaration"]
    }

    fn method_types(&self) -> &'static [&'static str] {
        &["method_definition", "public_field_definition"]
    }

    fn classify_method_node(&self, node: &SyntaxNode) -> MethodClassification {
        classify_ts_class_member(node)
    }

    fn interface_types(&self) -> &'static [&'static str] {
        &["interface_declaration"]
    }

    fn struct_types(&self) -> &'static [&'static str] {
        &[]
    }

    fn enum_types(&self) -> &'static [&'static str] {
        &["enum_declaration"]
    }

    fn enum_member_types(&self) -> &'static [&'static str] {
        &["property_identifier", "enum_assignment"]
    }

    fn type_alias_types(&self) -> &'static [&'static str] {
        &["type_alias_declaration"]
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

    fn body_field(&self) -> &'static str {
        "body"
    }

    fn params_field(&self) -> &'static str {
        "parameters"
    }

    fn return_field(&self) -> Option<&'static str> {
        Some("return_type")
    }

    fn resolve_body(&self, node: &SyntaxNode, body_field: &str) -> Option<SyntaxNode> {
        // class field 被判定为方法时，body 需要从初始化函数里取。
        field_function_body(node, "public_field_definition", body_field)
            .or_else(|| node.child_for_field_name(body_field).cloned())
    }

    fn get_signature(&self, node: &SyntaxNode, source: &str) -> Option<String> {
        if let Some(signature) = field_signature(node, source) {
            return Some(signature);
        }
        let params = get_child_by_field(node, "parameters")?;
        let mut sig = get_node_text(params, source);
        if let Some(return_type) = get_child_by_field(node, "return_type") {
            sig.push_str(": ");
            sig.push_str(
                get_node_text(return_type, source)
                    .trim_start_matches(':')
                    .trim_start(),
            );
        }
        Some(sig)
    }

    fn get_visibility(&self, node: &SyntaxNode) -> Option<Visibility> {
        for i in 0..node.child_count() {
            let Some(child) = node.child(i) else {
                continue;
            };
            if child.type_name() == "accessibility_modifier" {
                return match child.text().as_str() {
                    "public" => Some(Visibility::Public),
                    "private" => Some(Visibility::Private),
                    "protected" => Some(Visibility::Protected),
                    _ => None,
                };
            }
        }
        None
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

    fn is_static(&self, node: &SyntaxNode) -> bool {
        (0..node.child_count()).any(|i| node.child(i).is_some_and(|c| c.type_name() == "static"))
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
        extract_ts_js_variables(node, source, self.is_const(node))
    }
}

#[allow(dead_code)]
pub(crate) fn resolve_ts_field_body(
    node: &SyntaxNode,
    field_node_type: &str,
    body_field: &str,
) -> Option<SyntaxNode> {
    field_function_body(node, field_node_type, body_field)
}
