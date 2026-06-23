//! Rust translation of `src/extraction/languages/php.ts`.

use crate::extraction::tree_sitter_helpers::{get_child_by_field, get_node_text};
use crate::extraction::tree_sitter_types::{
    ClassClassification, ExtractorContext, ImportInfo, LanguageExtractor, NodeExtra,
    UnresolvedReferenceInput, Visibility,
};
use crate::types::{NodeKind, ReferenceKind};
use crate::web_tree_sitter::SyntaxNode;

pub struct PhpExtractor;

pub const PHP_EXTRACTOR: PhpExtractor = PhpExtractor;

pub fn php_extractor() -> &'static PhpExtractor {
    &PHP_EXTRACTOR
}

const PHP_INCLUDE_TYPES: &[&str] = &[
    "include_expression",
    "include_once_expression",
    "require_expression",
    "require_once_expression",
];

const PHP_NON_CLASS_RETURN: &[&str] = &[
    "array", "string", "int", "integer", "float", "double", "bool", "boolean", "void", "mixed",
    "never", "null", "false", "true", "object", "callable", "iterable", "resource",
];

fn php_static_include_path(node: &SyntaxNode, source: &str) -> Option<String> {
    let mut arg = node.named_child(0)?;
    if arg.type_name() == "parenthesized_expression" {
        arg = arg.named_child(0)?;
    }
    if arg.type_name() != "string" && arg.type_name() != "encapsed_string" {
        return None;
    }
    if arg
        .named_children()
        .into_iter()
        .any(|c| c.type_name() != "string_content")
    {
        // 动态拼接的 include 无法可靠定位文件，宁可不产出错误候选。
        return None;
    }
    arg.named_children()
        .into_iter()
        .find(|c| c.type_name() == "string_content")
        .map(|content| get_node_text(&content, source))
}

fn extract_php_return_type(node: &SyntaxNode, source: &str) -> Option<String> {
    let mut rt = get_child_by_field(node, "return_type")?;
    if rt.type_name() == "optional_type" {
        rt = rt.named_child(0).unwrap_or(rt);
    }
    if rt.type_name() == "primitive_type" {
        return None;
    }
    let name_node = if rt.type_name() == "named_type" {
        rt.named_child(0).unwrap_or(rt)
    } else {
        rt
    };
    let text = get_node_text(name_node, source)
        .trim()
        .trim_start_matches('\\')
        .to_string();
    if text.is_empty() {
        return None;
    }
    let last = text.split('\\').next_back().unwrap_or(&text).to_string();
    let lower = last.to_lowercase();
    if matches!(lower.as_str(), "self" | "static" | "this" | "$this") {
        // 自类型后续会结合当前类作用域解析，统一折叠成 self。
        return Some("self".to_string());
    }
    if PHP_NON_CLASS_RETURN.contains(&lower.as_str()) || !is_identifier(&last) {
        return None;
    }
    Some(last)
}

fn is_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some('_') | Some('A'..='Z') | Some('a'..='z'))
        && chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
}

fn php_grouped_use_names(node: &SyntaxNode, source: &str) -> Vec<String> {
    // `use Foo\{Bar, Baz}` 需要拆成多个 import 节点；通用 extract_import 只能返回一个。
    let Some(prefix_node) = node
        .named_children()
        .into_iter()
        .find(|c| c.type_name() == "namespace_name")
    else {
        return Vec::new();
    };
    let Some(group_node) = node
        .named_children()
        .into_iter()
        .find(|c| c.type_name() == "namespace_use_group")
    else {
        return Vec::new();
    };
    let prefix = get_node_text(&prefix_node, source);
    let mut names = Vec::new();
    let mut queue = group_node.named_children();
    while let Some(current) = queue.first().cloned() {
        queue.remove(0);
        if current.type_name() == "namespace_use_clause"
            && let Some(name_node) = current
                .named_children()
                .into_iter()
                .find(|c| c.type_name() == "name" || c.type_name() == "qualified_name")
        {
            names.push(format!("{prefix}\\{}", get_node_text(&name_node, source)));
            continue;
        }
        queue.extend(current.named_children());
    }
    names
}

impl LanguageExtractor for PhpExtractor {
    fn function_types(&self) -> &'static [&'static str] {
        &["function_definition"]
    }

    fn class_types(&self) -> &'static [&'static str] {
        &["class_declaration", "trait_declaration"]
    }

    fn method_types(&self) -> &'static [&'static str] {
        &["method_declaration"]
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
        &["enum_case"]
    }

    fn type_alias_types(&self) -> &'static [&'static str] {
        &[]
    }

    fn import_types(&self) -> &'static [&'static str] {
        &[
            "namespace_use_declaration",
            "include_expression",
            "include_once_expression",
            "require_expression",
            "require_once_expression",
        ]
    }

    fn call_types(&self) -> &'static [&'static str] {
        &[
            "function_call_expression",
            "member_call_expression",
            "scoped_call_expression",
        ]
    }

    fn variable_types(&self) -> &'static [&'static str] {
        &["const_declaration"]
    }

    fn field_types(&self) -> &'static [&'static str] {
        &["property_declaration"]
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

    fn get_return_type(&self, node: &SyntaxNode, source: &str) -> Option<String> {
        extract_php_return_type(node, source)
    }

    fn classify_class_node(&self, node: &SyntaxNode) -> ClassClassification {
        if node.type_name() == "trait_declaration" {
            ClassClassification::Trait
        } else {
            ClassClassification::Class
        }
    }

    fn get_visibility(&self, node: &SyntaxNode) -> Option<Visibility> {
        for i in 0..node.child_count() {
            let Some(child) = node.child(i) else {
                continue;
            };
            if child.type_name() == "visibility_modifier" {
                return match child.text().as_str() {
                    "public" => Some(Visibility::Public),
                    "private" => Some(Visibility::Private),
                    "protected" => Some(Visibility::Protected),
                    _ => None,
                };
            }
        }
        Some(Visibility::Public)
    }

    fn is_static(&self, node: &SyntaxNode) -> bool {
        (0..node.child_count()).any(|i| {
            node.child(i)
                .is_some_and(|c| c.type_name() == "static_modifier")
        })
    }

    fn visit_node(&self, node: &SyntaxNode, ctx: &mut dyn ExtractorContext) -> bool {
        if node.type_name() == "namespace_use_declaration" {
            let grouped_names = php_grouped_use_names(node, ctx.source());
            if !grouped_names.is_empty() {
                let import_text = get_node_text(node, ctx.source()).trim().to_string();
                let parent_id = ctx.node_stack().last().cloned();
                for name in grouped_names {
                    let import = ctx.create_node(
                        NodeKind::Import,
                        &name,
                        node,
                        NodeExtra {
                            signature: Some(import_text.clone()),
                            ..NodeExtra::default()
                        },
                    );
                    if let (Some(parent_id), Some(import)) = (parent_id.as_ref(), import.as_ref()) {
                        ctx.add_unresolved_reference(UnresolvedReferenceInput {
                            from_node_id: parent_id.clone(),
                            reference_name: import.name.clone(),
                            reference_kind: ReferenceKind::Imports,
                            file_path: Some(ctx.file_path().to_string()),
                            line: Some(node.start_position().row + 1),
                            column: Some(node.start_position().column),
                        });
                    }
                }
                return true;
            }
        }

        if node.type_name() == "const_declaration" {
            // 一个 const_declaration 可声明多个 const_element，必须逐个建常量节点。
            for elem in node
                .named_children()
                .into_iter()
                .filter(|c| c.type_name() == "const_element")
            {
                if let Some(name_node) = elem
                    .named_children()
                    .into_iter()
                    .find(|c| c.type_name() == "name")
                {
                    ctx.create_node(
                        NodeKind::Constant,
                        &get_node_text(&name_node, ctx.source()),
                        &elem,
                        NodeExtra::default(),
                    );
                }
            }
            return true;
        }

        if node.type_name() == "use_declaration" {
            // 类体内的 `use Trait` 是 trait composition，不是文件 import。
            let names = node
                .named_children()
                .into_iter()
                .filter(|c| c.type_name() == "name" || c.type_name() == "qualified_name")
                .collect::<Vec<_>>();
            if let Some(parent_id) = ctx.node_stack().last().cloned() {
                for name_node in names {
                    ctx.add_unresolved_reference(UnresolvedReferenceInput {
                        from_node_id: parent_id.clone(),
                        reference_name: get_node_text(&name_node, ctx.source()),
                        reference_kind: ReferenceKind::Implements,
                        file_path: Some(ctx.file_path().to_string()),
                        line: Some(node.start_position().row + 1),
                        column: Some(node.start_position().column),
                    });
                }
            }
            return true;
        }

        false
    }

    fn package_types(&self) -> &'static [&'static str] {
        &["namespace_definition"]
    }

    fn extract_package(&self, node: &SyntaxNode, source: &str) -> Option<String> {
        let ns_name = node
            .named_children()
            .into_iter()
            .find(|c| c.type_name() == "namespace_name")?;
        let has_body = node
            .named_children()
            .into_iter()
            .any(|c| c.type_name() == "compound_statement" || c.type_name() == "declaration_list");
        // 花括号 namespace 只作为作用域容器，避免生成一个覆盖整段文件的 package 节点。
        (!has_body).then(|| get_node_text(&ns_name, source))
    }

    fn extract_import(&self, node: &SyntaxNode, source: &str) -> Option<ImportInfo> {
        let import_text = get_node_text(node, source).trim().to_string();
        if PHP_INCLUDE_TYPES.contains(&node.type_name().as_str()) {
            return php_static_include_path(node, source).map(|module_name| ImportInfo {
                module_name,
                signature: import_text,
                handled_refs: false,
            });
        }

        let namespace_prefix = node
            .named_children()
            .into_iter()
            .find(|c| c.type_name() == "namespace_name");
        let use_group = node
            .named_children()
            .into_iter()
            .find(|c| c.type_name() == "namespace_use_group");
        if namespace_prefix.is_some() && use_group.is_some() {
            return None;
        }

        let use_clause = node
            .named_children()
            .into_iter()
            .find(|c| c.type_name() == "namespace_use_clause")?;
        if let Some(qualified_name) = use_clause
            .named_children()
            .into_iter()
            .find(|c| c.type_name() == "qualified_name")
        {
            return Some(ImportInfo {
                module_name: get_node_text(&qualified_name, source),
                signature: import_text,
                handled_refs: false,
            });
        }
        let name = use_clause
            .named_children()
            .into_iter()
            .find(|c| c.type_name() == "name")?;
        Some(ImportInfo {
            module_name: get_node_text(&name, source),
            signature: import_text,
            handled_refs: false,
        })
    }
}
