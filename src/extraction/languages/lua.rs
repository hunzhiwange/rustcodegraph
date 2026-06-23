//! Rust translation of `src/extraction/languages/lua.ts`.

use crate::extraction::tree_sitter_helpers::{get_child_by_field, get_node_text};
use crate::extraction::tree_sitter_types::{
    ExtractorContext, LanguageExtractor, NodeExtra, UnresolvedReferenceInput, VariableInfo,
};
use crate::types::{NodeKind, ReferenceKind};
use crate::web_tree_sitter::SyntaxNode;

pub struct LuaExtractor;

pub const LUA_EXTRACTOR: LuaExtractor = LuaExtractor;

pub fn lua_extractor() -> &'static LuaExtractor {
    &LUA_EXTRACTOR
}

fn find_descendant(node: &SyntaxNode, node_type: &str) -> Option<SyntaxNode> {
    let mut queue = node.named_children();
    while let Some(current) = queue.first().cloned() {
        queue.remove(0);
        if current.type_name() == node_type {
            return Some(current);
        }
        queue.extend(current.named_children());
    }
    None
}

fn require_module(call_node: &SyntaxNode, source: &str) -> Option<String> {
    let name = get_child_by_field(call_node, "name")?;
    if name.type_name() != "identifier" || get_node_text(name, source) != "require" {
        return None;
    }
    let args = get_child_by_field(call_node, "arguments")?;

    if let Some(content) = find_descendant(args, "string_content") {
        let module_name = get_node_text(&content, source).trim().to_string();
        return (!module_name.is_empty()).then_some(module_name);
    }
    if let Some(str_node) = find_descendant(args, "string") {
        let module_name = get_node_text(&str_node, source)
            .trim()
            .trim_start_matches("[[")
            .trim_end_matches("]]")
            .trim_matches(['"', '\''])
            .to_string();
        if !module_name.is_empty() {
            return Some(module_name);
        }
    }

    let idx = find_descendant(args, "dot_index_expression")
        .or_else(|| find_descendant(args, "method_index_expression"))?;
    // 兼容 `require(foo.bar)` 一类非字符串形式，只取最终字段作为模块名候选。
    let field = get_child_by_field(&idx, "field").or_else(|| get_child_by_field(&idx, "method"))?;
    let module_name = get_node_text(field, source).trim().to_string();
    (!module_name.is_empty()).then_some(module_name)
}

fn emit_require_import(call_node: &SyntaxNode, ctx: &mut dyn ExtractorContext) {
    let Some(module_name) = require_module(call_node, ctx.source()) else {
        return;
    };
    let extra = NodeExtra {
        signature: Some(
            get_node_text(call_node, ctx.source())
                .trim()
                .chars()
                .take(100)
                .collect(),
        ),
        ..Default::default()
    };
    let import = ctx.create_node(NodeKind::Import, &module_name, call_node, extra);
    if import.is_some()
        && let Some(parent_id) = ctx.node_stack().last()
    {
        // Lua 没有声明式 import 节点，`require` 调用同时创建 import 节点和 unresolved import 边。
        ctx.add_unresolved_reference(UnresolvedReferenceInput {
            from_node_id: parent_id.clone(),
            reference_name: module_name,
            reference_kind: ReferenceKind::Imports,
            file_path: None,
            line: Some(call_node.start_position().row + 1),
            column: Some(call_node.start_position().column),
        });
    }
}

fn variable_list_names(variable_list: &SyntaxNode, source: &str) -> Vec<(String, SyntaxNode)> {
    variable_list
        .named_children()
        .into_iter()
        .filter(|child| matches!(child.type_name().as_str(), "variable" | "identifier"))
        .filter_map(|variable| {
            let name = get_node_text(&variable, source).trim().to_string();
            (!name.is_empty()).then_some((name, variable))
        })
        .collect()
}

fn expression_values(expression_list: Option<SyntaxNode>) -> Vec<SyntaxNode> {
    expression_list
        .map(|list| list.named_children())
        .unwrap_or_default()
}

fn assignment_parts(node: &SyntaxNode) -> (Option<SyntaxNode>, Option<SyntaxNode>) {
    // Lua 的 local 声明和普通赋值 AST 不一致，统一拆成变量列表和值列表后按位置配对。
    let assignment = if node.type_name() == "assignment_statement" {
        Some(node.clone())
    } else {
        node.named_children()
            .into_iter()
            .find(|child| child.type_name() == "assignment_statement")
    };
    let Some(assignment) = assignment else {
        let variables = node
            .named_children()
            .into_iter()
            .find(|child| child.type_name() == "variable_list");
        return (variables, None);
    };
    let variables = assignment
        .named_children()
        .into_iter()
        .find(|child| child.type_name() == "variable_list");
    let values = assignment
        .named_children()
        .into_iter()
        .find(|child| child.type_name() == "expression_list");
    (variables, values)
}

impl LanguageExtractor for LuaExtractor {
    fn function_types(&self) -> &'static [&'static str] {
        &["function_declaration"]
    }

    fn class_types(&self) -> &'static [&'static str] {
        &[]
    }

    fn method_types(&self) -> &'static [&'static str] {
        &[]
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
        &[]
    }

    fn call_types(&self) -> &'static [&'static str] {
        &["function_call"]
    }

    fn variable_types(&self) -> &'static [&'static str] {
        &["variable_declaration"]
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

    fn get_signature(&self, node: &SyntaxNode, source: &str) -> Option<String> {
        get_child_by_field(node, "parameters").map(|params| get_node_text(params, source))
    }

    fn get_receiver_type(&self, node: &SyntaxNode, source: &str) -> Option<String> {
        let name = get_child_by_field(node, "name")?;
        // `function table:method()` 和 `function table.method()` 都把 receiver 放在 name 子树里。
        if name.type_name() == "dot_index_expression"
            || name.type_name() == "method_index_expression"
        {
            return get_child_by_field(name, "table").map(|table| get_node_text(table, source));
        }
        None
    }

    fn visit_node(&self, node: &SyntaxNode, ctx: &mut dyn ExtractorContext) -> bool {
        if node.type_name() == "function_call" {
            if require_module(node, ctx.source()).is_some() {
                emit_require_import(node, ctx);
                return true;
            }
            return false;
        }

        false
    }

    fn extract_variables(&self, node: &SyntaxNode, source: &str) -> Vec<VariableInfo> {
        let (variables, values) = assignment_parts(node);
        let Some(variables) = variables else {
            return Vec::new();
        };
        let values = expression_values(values);
        variable_list_names(&variables, source)
            .into_iter()
            .enumerate()
            .map(|(idx, (name, position_node))| VariableInfo {
                name,
                kind: NodeKind::Variable,
                signature: None,
                is_exported: Some(false),
                delegate_to_function: None,
                position_node: Some(position_node),
                visit_value: values.get(idx).cloned(),
                object_literal_functions: None,
            })
            .collect()
    }
}
