//! Rust translation of `src/extraction/languages/python.ts`.

use crate::extraction::tree_sitter_helpers::{get_child_by_field, get_node_text};
use crate::extraction::tree_sitter_types::{
    ExtractorContext, ImportInfo, LanguageExtractor, UnresolvedReferenceInput,
};
use crate::types::ReferenceKind;
use crate::web_tree_sitter::SyntaxNode;

pub struct PythonExtractor;

pub const PYTHON_EXTRACTOR: PythonExtractor = PythonExtractor;

pub fn python_extractor() -> &'static PythonExtractor {
    &PYTHON_EXTRACTOR
}

fn first_python_import_name(node: &SyntaxNode, source: &str) -> Option<String> {
    // `import a, b as c` 这里只取第一个模块作为 import 节点名；
    // from-import 的具体导入项会在 visit_node 里补 unresolved refs。
    for child in node.named_children() {
        match child.type_name().as_str() {
            "dotted_name" | "identifier" => return Some(get_node_text(&child, source)),
            "aliased_import" => {
                if let Some(name) = child
                    .named_children()
                    .into_iter()
                    .find(|c| c.type_name() == "dotted_name" || c.type_name() == "identifier")
                {
                    return Some(get_node_text(&name, source));
                }
            }
            _ => {}
        }
    }
    None
}

impl LanguageExtractor for PythonExtractor {
    fn function_types(&self) -> &'static [&'static str] {
        &["function_definition"]
    }

    fn class_types(&self) -> &'static [&'static str] {
        &["class_definition"]
    }

    fn method_types(&self) -> &'static [&'static str] {
        &["function_definition"]
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
        &["import_statement", "import_from_statement"]
    }

    fn call_types(&self) -> &'static [&'static str] {
        &["call"]
    }

    fn variable_types(&self) -> &'static [&'static str] {
        &["assignment"]
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

    fn get_signature(&self, node: &SyntaxNode, source: &str) -> Option<String> {
        let params = get_child_by_field(node, "parameters")?;
        let mut sig = get_node_text(params, source);
        if let Some(return_type) = get_child_by_field(node, "return_type") {
            sig.push_str(" -> ");
            sig.push_str(&get_node_text(return_type, source));
        }
        Some(sig)
    }

    fn is_async(&self, node: &SyntaxNode) -> bool {
        node.previous_sibling()
            .is_some_and(|prev| prev.type_name() == "async")
    }

    fn is_static(&self, node: &SyntaxNode) -> bool {
        node.previous_named_sibling().is_some_and(|prev| {
            prev.type_name() == "decorator" && prev.text().contains("staticmethod")
        })
    }

    fn visit_node(&self, node: &SyntaxNode, ctx: &mut dyn ExtractorContext) -> bool {
        if node.type_name() != "import_from_statement" {
            return false;
        }
        // `from module import symbol` 同时需要模块 import 和每个 symbol 的引用，
        // 否则 resolver 无法把本文件使用的类/函数连到导出方。
        let import_text = get_node_text(node, ctx.source()).trim().to_string();
        let Some((_, imported)) = import_text.split_once(" import ") else {
            return false;
        };
        let Some(parent_id) = ctx.node_stack().last().cloned() else {
            return false;
        };
        for raw in imported.split(',') {
            let item = raw
                .trim()
                .trim_matches(['(', ')'])
                .split(" as ")
                .next()
                .unwrap_or("")
                .trim();
            if item.is_empty() || item == "*" {
                continue;
            }
            ctx.add_unresolved_reference(UnresolvedReferenceInput {
                from_node_id: parent_id.clone(),
                reference_name: item.to_string(),
                reference_kind: ReferenceKind::Imports,
                file_path: Some(ctx.file_path().to_string()),
                line: Some(node.start_position().row + 1),
                column: Some(node.start_position().column),
            });
        }
        false
    }

    fn extract_import(&self, node: &SyntaxNode, source: &str) -> Option<ImportInfo> {
        let import_text = get_node_text(node, source).trim().to_string();
        if node.type_name() == "import_from_statement" {
            // 优先按源码文本切出 module，兼容相对导入和 grammar 字段缺失的错误恢复。
            let module_name = import_text
                .strip_prefix("from ")
                .and_then(|rest| rest.split(" import").next())
                .filter(|module| !module.trim().is_empty())
                .map(|module| module.trim().to_string())
                .or_else(|| {
                    node.child_for_field_name("module_name")
                        .map(|module| get_node_text(module, source))
                })?;
            return Some(ImportInfo {
                module_name,
                signature: import_text,
                handled_refs: false,
            });
        }

        if node.type_name() == "import_statement" {
            return first_python_import_name(node, source).map(|module_name| ImportInfo {
                module_name,
                signature: import_text,
                handled_refs: false,
            });
        }

        None
    }
}
