//! Rust translation of `src/extraction/languages/csharp.ts`.

use crate::extraction::tree_sitter_helpers::get_node_text;
use crate::extraction::tree_sitter_types::{
    ClassClassification, ImportInfo, LanguageExtractor, Visibility,
};
use crate::web_tree_sitter::SyntaxNode;

pub struct CsharpExtractor;

pub const CSHARP_EXTRACTOR: CsharpExtractor = CsharpExtractor;

pub fn csharp_extractor() -> &'static CsharpExtractor {
    &CSHARP_EXTRACTOR
}

/// Blank C# conditional-compilation directive lines while preserving byte offsets.
pub fn blank_csharp_preprocessor_directives(source: &str) -> String {
    if !source.contains('#') {
        return source.to_string();
    }

    let mut out = String::with_capacity(source.len());
    for line in source.split_inclusive('\n') {
        let without_newline = line.strip_suffix('\n').unwrap_or(line);
        let indent_len = without_newline
            .chars()
            .take_while(|c| *c == ' ' || *c == '\t')
            .map(char::len_utf8)
            .sum::<usize>();
        let trimmed = &without_newline[indent_len..];
        let is_conditional = ["#if", "#elif", "#else", "#endif"]
            .iter()
            .any(|prefix| trimmed.starts_with(prefix));
        if is_conditional {
            // 条件编译会让 tree-sitter 在半开分支上误恢复；用空格替换而非删除，
            // 保证后续节点的 byte/line 位置还能映射回原文件。
            out.push_str(&without_newline[..indent_len]);
            out.push_str(&" ".repeat(without_newline.len() - indent_len));
            if line.ends_with('\n') {
                out.push('\n');
            }
        } else {
            out.push_str(line);
        }
    }
    out
}

fn extract_csharp_return_type(node: &SyntaxNode, source: &str) -> Option<String> {
    let type_node = node.child_for_field_name("returns")?;
    // 只返回可能参与成员调用解析的类名；基础类型和数组不会提供有价值的接收者。
    if type_node.type_name() == "predefined_type" || type_node.type_name() == "array_type" {
        return None;
    }
    let mut t = get_node_text(type_node, source)
        .trim()
        .trim_end_matches('?')
        .to_string();
    t = strip_angle_generics(&t);
    let last = t.split('.').next_back()?.trim();
    is_identifier(last).then(|| last.to_string())
}

fn strip_angle_generics(input: &str) -> String {
    let mut out = String::new();
    let mut depth = 0usize;
    for ch in input.chars() {
        match ch {
            '<' => depth += 1,
            '>' => depth = depth.saturating_sub(1),
            _ if depth == 0 => out.push(ch),
            _ => {}
        }
    }
    out
}

fn is_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some('_') | Some('A'..='Z') | Some('a'..='z'))
        && chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
}

impl LanguageExtractor for CsharpExtractor {
    fn pre_parse(&self, source: &str) -> Option<String> {
        Some(blank_csharp_preprocessor_directives(source))
    }

    fn function_types(&self) -> &'static [&'static str] {
        &[]
    }

    fn class_types(&self) -> &'static [&'static str] {
        &["class_declaration", "record_declaration"]
    }

    fn method_types(&self) -> &'static [&'static str] {
        &["method_declaration", "constructor_declaration"]
    }

    fn interface_types(&self) -> &'static [&'static str] {
        &["interface_declaration"]
    }

    fn struct_types(&self) -> &'static [&'static str] {
        &["struct_declaration", "record_struct_declaration"]
    }

    fn classify_class_node(&self, node: &SyntaxNode) -> ClassClassification {
        // C# record struct 在 grammar 中仍可能落在 record_declaration，
        // 需要看子 token 才能映射到 RustCodeGraph 的 struct kind。
        if node.type_name() == "record_declaration"
            && (0..node.child_count())
                .any(|i| node.child(i).is_some_and(|c| c.type_name() == "struct"))
        {
            ClassClassification::Struct
        } else {
            ClassClassification::Class
        }
    }

    fn enum_types(&self) -> &'static [&'static str] {
        &["enum_declaration"]
    }

    fn enum_member_types(&self) -> &'static [&'static str] {
        &["enum_member_declaration"]
    }

    fn type_alias_types(&self) -> &'static [&'static str] {
        &[]
    }

    fn package_types(&self) -> &'static [&'static str] {
        &["namespace_declaration", "file_scoped_namespace_declaration"]
    }

    fn extract_package(&self, node: &SyntaxNode, source: &str) -> Option<String> {
        node.child_for_field_name("name")
            .cloned()
            .or_else(|| {
                node.named_children()
                    .into_iter()
                    .find(|c| c.type_name() == "qualified_name" || c.type_name() == "identifier")
            })
            .map(|name| get_node_text(&name, source))
    }

    fn import_types(&self) -> &'static [&'static str] {
        &["using_directive"]
    }

    fn call_types(&self) -> &'static [&'static str] {
        &["invocation_expression"]
    }

    fn variable_types(&self) -> &'static [&'static str] {
        &["local_declaration_statement"]
    }

    fn field_types(&self) -> &'static [&'static str] {
        &["field_declaration"]
    }

    fn property_types(&self) -> &'static [&'static str] {
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
        Some("type")
    }

    fn get_return_type(&self, node: &SyntaxNode, source: &str) -> Option<String> {
        extract_csharp_return_type(node, source)
    }

    fn get_visibility(&self, node: &SyntaxNode) -> Option<Visibility> {
        for i in 0..node.child_count() {
            let Some(child) = node.child(i) else {
                continue;
            };
            if child.type_name() == "modifier" {
                return match child.text().as_str() {
                    "public" => Some(Visibility::Public),
                    "private" => Some(Visibility::Private),
                    "protected" => Some(Visibility::Protected),
                    "internal" => Some(Visibility::Internal),
                    _ => None,
                };
            }
        }
        // C# 成员默认 private；这里显式返回，避免查询层把缺省可见性误当未知。
        Some(Visibility::Private)
    }

    fn is_static(&self, node: &SyntaxNode) -> bool {
        (0..node.child_count()).any(|i| {
            node.child(i)
                .is_some_and(|c| c.type_name() == "modifier" && c.text() == "static")
        })
    }

    fn is_const(&self, node: &SyntaxNode) -> bool {
        let mut has_static = false;
        let mut has_readonly = false;
        for i in 0..node.child_count() {
            let Some(child) = node.child(i) else {
                continue;
            };
            if child.type_name() != "modifier" {
                continue;
            }
            match child.text().as_str() {
                "const" => return true,
                "static" => has_static = true,
                "readonly" => has_readonly = true,
                _ => {}
            }
        }
        // `static readonly` 对调用方近似常量，保留下来能让字段摘要更贴近 C# 惯例。
        has_static && has_readonly
    }

    fn is_async(&self, node: &SyntaxNode) -> bool {
        (0..node.child_count()).any(|i| {
            node.child(i)
                .is_some_and(|c| c.type_name() == "modifier" && c.text() == "async")
        })
    }

    fn extract_import(&self, node: &SyntaxNode, source: &str) -> Option<ImportInfo> {
        let import_text = get_node_text(node, source).trim().to_string();
        if let Some(qualified_name) = node
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
        let identifier = node
            .named_children()
            .into_iter()
            .find(|c| c.type_name() == "identifier")?;
        Some(ImportInfo {
            module_name: get_node_text(&identifier, source),
            signature: import_text,
            handled_refs: false,
        })
    }
}
