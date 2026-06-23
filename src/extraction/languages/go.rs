//! Rust translation of `src/extraction/languages/go.ts`.

use crate::extraction::tree_sitter_helpers::{get_child_by_field, get_node_text};
use crate::extraction::tree_sitter_types::{
    ExtractorContext, LanguageExtractor, NodeExtra, UnresolvedReferenceInput,
};
use crate::types::{NodeKind, ReferenceKind};
use crate::web_tree_sitter::SyntaxNode;

pub struct GoExtractor;

pub const GO_EXTRACTOR: GoExtractor = GoExtractor;

pub fn go_extractor() -> &'static GoExtractor {
    &GO_EXTRACTOR
}

fn extract_go_return_type(node: &SyntaxNode, source: &str) -> Option<String> {
    let mut result = get_child_by_field(node, "result")?.clone();
    // Go 的返回值可以是 `(name Type)` 形式；解析调用接收者时只关心真实类型节点。
    if result.type_name() == "parameter_list" {
        let first = result
            .named_children()
            .into_iter()
            .find(|c| c.type_name() == "parameter_declaration")?;
        result = get_child_by_field(&first, "type").cloned().unwrap_or(first);
    }
    if result.type_name() == "pointer_type"
        && let Some(inner) = result.named_children().into_iter().find(|c| {
            c.type_name() == "type_identifier"
                || c.type_name() == "qualified_type"
                || c.type_name() == "generic_type"
        })
    {
        result = inner;
    }

    let text = get_node_text(&result, source)
        .trim()
        .trim_start_matches('*')
        .replace(['<', '>'], "");
    let text = strip_square_generics(&text);
    let last = text.split('.').next_back()?.trim();
    is_identifier(last).then(|| last.to_string())
}

fn strip_square_generics(input: &str) -> String {
    let mut out = String::new();
    let mut depth = 0usize;
    for ch in input.chars() {
        match ch {
            '[' => depth += 1,
            ']' => depth = depth.saturating_sub(1),
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

fn find_go_string_literal(node: &SyntaxNode) -> Option<SyntaxNode> {
    let mut queue = node.named_children();
    while let Some(current) = queue.first().cloned() {
        queue.remove(0);
        if current.type_name().ends_with("string_literal") {
            return Some(current);
        }
        queue.extend(current.named_children());
    }
    None
}

fn go_import_path(spec: &SyntaxNode, source: &str) -> Option<String> {
    let literal = find_go_string_literal(spec)?;
    let path = get_node_text(&literal, source)
        .trim()
        .trim_matches(['"', '`'])
        .to_string();
    (!path.is_empty()).then_some(path)
}

fn go_import_specs(node: &SyntaxNode) -> Vec<SyntaxNode> {
    // `import "x"` 和 `import (...)` 在 AST 上层级不同，统一摊平成 spec 列表后手动建 import 节点。
    if node.type_name() == "import_spec" {
        return vec![node.clone()];
    }
    let mut specs = Vec::new();
    let mut queue = node.named_children();
    while let Some(current) = queue.first().cloned() {
        queue.remove(0);
        if current.type_name() == "import_spec" {
            specs.push(current);
        } else {
            queue.extend(current.named_children());
        }
    }
    specs
}

impl LanguageExtractor for GoExtractor {
    fn function_types(&self) -> &'static [&'static str] {
        &["function_declaration"]
    }

    fn class_types(&self) -> &'static [&'static str] {
        &[]
    }

    fn method_types(&self) -> &'static [&'static str] {
        &["method_declaration"]
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
        &["type_spec"]
    }

    fn import_types(&self) -> &'static [&'static str] {
        &["import_declaration"]
    }

    fn call_types(&self) -> &'static [&'static str] {
        &["call_expression"]
    }

    fn variable_types(&self) -> &'static [&'static str] {
        &[
            "var_declaration",
            "short_var_declaration",
            "const_declaration",
        ]
    }

    fn visit_node(&self, node: &SyntaxNode, ctx: &mut dyn ExtractorContext) -> bool {
        if node.type_name() != "import_declaration" {
            return false;
        }

        // 通用 import 抽取只能得到一个 declaration；Go 需要为每个 spec 建单独节点和 unresolved import。
        let parent_id = ctx.node_stack().last().cloned();
        for spec in go_import_specs(node) {
            let Some(module_name) = go_import_path(&spec, ctx.source()) else {
                continue;
            };
            let import = ctx.create_node(
                NodeKind::Import,
                &module_name,
                &spec,
                NodeExtra {
                    signature: Some(get_node_text(&spec, ctx.source()).trim().to_string()),
                    ..NodeExtra::default()
                },
            );
            if let (Some(parent_id), Some(import)) = (parent_id.as_ref(), import.as_ref()) {
                ctx.add_unresolved_reference(UnresolvedReferenceInput {
                    from_node_id: parent_id.clone(),
                    reference_name: import.name.clone(),
                    reference_kind: ReferenceKind::Imports,
                    file_path: Some(ctx.file_path().to_string()),
                    line: Some(spec.start_position().row + 1),
                    column: Some(spec.start_position().column),
                });
            }
        }
        true
    }

    fn methods_are_top_level(&self) -> bool {
        // Go 方法的 receiver 在签名里，AST 上不是嵌套在类型体内；作用域由 receiver_type 后续补齐。
        true
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
        Some("result")
    }

    fn get_return_type(&self, node: &SyntaxNode, source: &str) -> Option<String> {
        extract_go_return_type(node, source)
    }

    fn get_signature(&self, node: &SyntaxNode, source: &str) -> Option<String> {
        let params = get_child_by_field(node, "parameters")?;
        let mut sig = get_node_text(params, source);
        if let Some(result) = get_child_by_field(node, "result") {
            sig.push(' ');
            sig.push_str(&get_node_text(result, source));
        }
        Some(sig)
    }

    fn resolve_type_alias_kind(&self, node: &SyntaxNode, _source: &str) -> Option<NodeKind> {
        let type_child = get_child_by_field(node, "type")?;
        match type_child.type_name().as_str() {
            "struct_type" => Some(NodeKind::Struct),
            "interface_type" => Some(NodeKind::Interface),
            _ => None,
        }
    }

    fn is_exported(&self, node: &SyntaxNode, source: &str) -> bool {
        get_child_by_field(node, "name")
            .map(|name| get_node_text(name, source))
            .and_then(|text| text.as_bytes().first().copied())
            .is_some_and(|first| first.is_ascii_uppercase())
    }

    fn get_receiver_type(&self, node: &SyntaxNode, source: &str) -> Option<String> {
        let receiver = get_child_by_field(node, "receiver")?;
        let text = get_node_text(receiver, source);
        receiver_name_from_go_receiver(&text)
    }
}

fn receiver_name_from_go_receiver(text: &str) -> Option<String> {
    // 兼容 `(r *Receiver)`、`(*Receiver)` 等形式，只留下解析边要用的类型名。
    let inner = text.trim().trim_start_matches('(');
    let inner = inner.trim_start();
    let inner = inner
        .split_whitespace()
        .last()
        .unwrap_or(inner)
        .trim_start_matches('*');
    let name = inner
        .chars()
        .take_while(|c| *c == '_' || c.is_ascii_alphanumeric())
        .collect::<String>();
    (!name.is_empty()).then_some(name)
}
