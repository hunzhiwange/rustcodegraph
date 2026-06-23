//! Rust translation of `src/extraction/languages/r.ts`.

use crate::extraction::tree_sitter_helpers::{get_child_by_field, get_node_text};
use crate::extraction::tree_sitter_types::{
    ExtractorContext, LanguageExtractor, NodeExtra, UnresolvedReferenceInput,
};
use crate::types::{NodeKind, ReferenceKind};
use crate::web_tree_sitter::SyntaxNode;

pub struct RExtractor;

pub const R_EXTRACTOR: RExtractor = RExtractor;

pub fn r_extractor() -> &'static RExtractor {
    &R_EXTRACTOR
}

const ASSIGN_LEFT: &[&str] = &["<-", "<<-", "="];
const ASSIGN_RIGHT: &[&str] = &["->", "->>"];
const IMPORT_FNS: &[&str] = &["library", "require", "requireNamespace", "loadNamespace"];
const CLASS_FNS: &[&str] = &["setClass", "setRefClass", "R6Class", "ggproto"];
const GENERIC_FNS: &[&str] = &["setGeneric", "setMethod"];

fn callee_name(call: &SyntaxNode, source: &str) -> Option<String> {
    // R 的调用目标可能是裸函数，也可能是 pkg::fn；只取 RHS 作为语义入口。
    let function = get_child_by_field(call, "function")?;
    if function.type_name() == "identifier" {
        return Some(get_node_text(function, source));
    }
    if function.type_name() == "namespace_operator" {
        return get_child_by_field(function, "rhs").map(|rhs| get_node_text(rhs, source));
    }
    None
}

fn first_arg_value(call: &SyntaxNode) -> Option<SyntaxNode> {
    let args = get_child_by_field(call, "arguments")?;
    for i in 0..args.named_child_count() {
        let Some(arg) = args.named_child(i) else {
            continue;
        };
        if arg.type_name() == "argument" {
            return get_child_by_field(arg, "value").cloned();
        }
    }
    None
}

fn literal_or_identifier(node: Option<SyntaxNode>, source: &str) -> Option<String> {
    let node = node?;
    if node.type_name() == "identifier" {
        return Some(get_node_text(&node, source));
    }
    if node.type_name() == "string" {
        return node
            .named_children()
            .into_iter()
            .find(|c| c.type_name() == "string_content")
            .map(|c| get_node_text(&c, source))
            .or_else(|| Some(String::new()));
    }
    None
}

fn emit_method_arg(entry: &SyntaxNode, ctx: &mut dyn ExtractorContext) {
    let entry_name = get_child_by_field(entry, "name");
    let entry_value = get_child_by_field(entry, "value");
    let (Some(entry_name), Some(entry_value)) = (entry_name, entry_value) else {
        return;
    };
    if entry_value.type_name() != "function_definition" {
        return;
    }
    // R6/ggproto 常把方法放在 list(name = function(...)) 参数里，手动建 method 并递归访问 body。
    let params = get_child_by_field(entry_value, "parameters");
    let extra = NodeExtra {
        signature: params.map(|params| get_node_text(params, ctx.source())),
        ..Default::default()
    };
    let method = ctx.create_node(
        NodeKind::Method,
        &get_node_text(entry_name, ctx.source()),
        entry,
        extra,
    );
    let body = get_child_by_field(entry_value, "body");
    if let (Some(method), Some(body)) = (method, body) {
        ctx.push_scope(method.id.clone());
        ctx.visit_node(body);
        ctx.pop_scope();
    }
}

fn extract_class_members(class_call: &SyntaxNode, class_id: &str, ctx: &mut dyn ExtractorContext) {
    let Some(args) = get_child_by_field(class_call, "arguments") else {
        return;
    };
    let mut positional = 0usize;
    for i in 0..args.named_child_count() {
        let Some(arg) = args.named_child(i) else {
            continue;
        };
        if arg.type_name() != "argument" {
            continue;
        }
        let arg_name = get_child_by_field(arg, "name");
        let value = get_child_by_field(arg, "value");
        if arg_name.is_none() {
            positional += 1;
            // setClass 第二个位置参数可表达继承关系。
            if positional == 2
                && value
                    .as_ref()
                    .is_some_and(|v| v.type_name() == "identifier")
            {
                let value = value.unwrap();
                ctx.add_unresolved_reference(UnresolvedReferenceInput {
                    from_node_id: class_id.to_string(),
                    reference_name: get_node_text(value, ctx.source()),
                    reference_kind: ReferenceKind::Extends,
                    file_path: None,
                    line: Some(value.start_position().row + 1),
                    column: Some(value.start_position().column),
                });
            }
            continue;
        }

        let arg_name_text = get_node_text(arg_name.unwrap(), ctx.source());
        if (arg_name_text == "inherit" || arg_name_text == "contains")
            && let Some(value) = value
        {
            if let Some(parent) = literal_or_identifier(Some(value.clone()), ctx.source()) {
                ctx.add_unresolved_reference(UnresolvedReferenceInput {
                    from_node_id: class_id.to_string(),
                    reference_name: parent,
                    reference_kind: ReferenceKind::Extends,
                    file_path: None,
                    line: Some(value.start_position().row + 1),
                    column: Some(value.start_position().column),
                });
            }
            continue;
        }

        if value
            .as_ref()
            .is_some_and(|v| v.type_name() == "function_definition")
        {
            emit_method_arg(arg, ctx);
            continue;
        }

        if value.as_ref().is_some_and(|v| {
            v.type_name() == "call" && callee_name(v, ctx.source()).as_deref() == Some("list")
        }) {
            let value = value.unwrap();
            let Some(list_args) = get_child_by_field(value, "arguments") else {
                continue;
            };
            for entry in list_args.named_children() {
                if entry.type_name() == "argument" {
                    emit_method_arg(&entry, ctx);
                }
            }
        }
    }
}

fn is_constant_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    first.is_ascii_uppercase()
        && chars.all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '.' || c == '_')
}

impl LanguageExtractor for RExtractor {
    fn function_types(&self) -> &'static [&'static str] {
        &[]
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
        &["call"]
    }

    fn variable_types(&self) -> &'static [&'static str] {
        &[]
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

    fn visit_node(&self, node: &SyntaxNode, ctx: &mut dyn ExtractorContext) -> bool {
        if node.type_name() == "call" {
            return visit_r_call(node, ctx);
        }
        if node.type_name() == "binary_operator" {
            return visit_r_binary_operator(node, ctx);
        }
        false
    }
}

fn visit_r_call(node: &SyntaxNode, ctx: &mut dyn ExtractorContext) -> bool {
    let Some(fname) = callee_name(node, ctx.source()) else {
        return false;
    };

    if IMPORT_FNS.contains(&fname.as_str()) || fname == "source" {
        // R 的包加载和 source 都是运行时调用，这里升格为 import 节点帮助跨文件/包追踪。
        let Some(module_name) = literal_or_identifier(first_arg_value(node), ctx.source()) else {
            return true;
        };
        let extra = NodeExtra {
            signature: Some(
                get_node_text(node, ctx.source())
                    .trim()
                    .chars()
                    .take(100)
                    .collect(),
            ),
            ..Default::default()
        };
        let import = ctx.create_node(NodeKind::Import, &module_name, node, extra);
        if import.is_some()
            && let Some(parent_id) = ctx.node_stack().last()
        {
            ctx.add_unresolved_reference(UnresolvedReferenceInput {
                from_node_id: parent_id.clone(),
                reference_name: module_name,
                reference_kind: ReferenceKind::Imports,
                file_path: None,
                line: Some(node.start_position().row + 1),
                column: Some(node.start_position().column),
            });
        }
        return true;
    }

    if CLASS_FNS.contains(&fname.as_str()) {
        // S4/R6/ggproto 类是函数调用声明，不会出现在普通 class_types 里。
        let Some(name) = literal_or_identifier(first_arg_value(node), ctx.source()) else {
            return false;
        };
        let class = ctx.create_node(NodeKind::Class, &name, node, NodeExtra::default());
        if let Some(class) = class {
            ctx.push_scope(class.id.clone());
            extract_class_members(node, &class.id, ctx);
            ctx.pop_scope();
        }
        return true;
    }

    if GENERIC_FNS.contains(&fname.as_str()) {
        let Some(name) = literal_or_identifier(first_arg_value(node), ctx.source()) else {
            return false;
        };
        let args = get_child_by_field(node, "arguments");
        let mut implementation = None;
        if let Some(args) = args {
            for arg in args.named_children() {
                let value = if arg.type_name() == "argument" {
                    get_child_by_field(&arg, "value").cloned()
                } else {
                    None
                };
                if value
                    .as_ref()
                    .is_some_and(|v| v.type_name() == "function_definition")
                {
                    implementation = value;
                    break;
                }
            }
        }
        let params = implementation
            .as_ref()
            .and_then(|implementation| get_child_by_field(implementation, "parameters"));
        let extra = NodeExtra {
            signature: params.map(|params| get_node_text(params, ctx.source())),
            ..Default::default()
        };
        let function = ctx.create_node(NodeKind::Function, &name, node, extra);
        let body = implementation
            .as_ref()
            .and_then(|implementation| get_child_by_field(implementation, "body"));
        if let (Some(function), Some(body)) = (function, body) {
            ctx.push_scope(function.id.clone());
            ctx.visit_node(body);
            ctx.pop_scope();
        }
        return true;
    }

    false
}

fn visit_r_binary_operator(node: &SyntaxNode, ctx: &mut dyn ExtractorContext) -> bool {
    let Some(op) = node.child_for_field_name("operator").map(|op| op.text()) else {
        return false;
    };
    let lhs = get_child_by_field(node, "lhs");
    let rhs = get_child_by_field(node, "rhs");

    if ASSIGN_LEFT.contains(&op.as_str())
        && lhs
            .as_ref()
            .is_some_and(|lhs| lhs.type_name() == "identifier")
        && rhs
            .as_ref()
            .is_some_and(|rhs| rhs.type_name() == "function_definition")
    {
        // `foo <- function(...)` 是 R 最常见的函数声明形式。
        let (Some(lhs), Some(rhs)) = (lhs, rhs) else {
            return false;
        };
        let params = get_child_by_field(rhs, "parameters");
        let extra = NodeExtra {
            signature: params.map(|params| get_node_text(params, ctx.source())),
            ..Default::default()
        };
        let function = ctx.create_node(
            NodeKind::Function,
            &get_node_text(lhs, ctx.source()),
            node,
            extra,
        );
        let body = get_child_by_field(rhs, "body");
        if let (Some(function), Some(body)) = (function, body) {
            ctx.push_scope(function.id.clone());
            ctx.visit_node(body);
            ctx.pop_scope();
        }
        return true;
    }

    let top_level = node
        .parent()
        .is_some_and(|parent| parent.type_name() == "program");
    if top_level
        && ASSIGN_LEFT.contains(&op.as_str())
        && lhs
            .as_ref()
            .is_some_and(|lhs| lhs.type_name() == "identifier")
        && rhs.is_some()
    {
        // 顶层赋值建变量/常量节点，但 class/generic 构造调用本身会在 visit_r_call 中建更具体节点。
        let (Some(lhs), Some(rhs)) = (lhs, rhs) else {
            return false;
        };
        let rhs_callee = (rhs.type_name() == "call")
            .then(|| callee_name(rhs, ctx.source()))
            .flatten();
        if !rhs_callee.as_ref().is_some_and(|callee| {
            CLASS_FNS.contains(&callee.as_str()) || GENERIC_FNS.contains(&callee.as_str())
        }) {
            let name = get_node_text(lhs, ctx.source());
            ctx.create_node(
                if is_constant_name(&name) {
                    NodeKind::Constant
                } else {
                    NodeKind::Variable
                },
                &name,
                node,
                NodeExtra::default(),
            );
        }
        ctx.visit_node(rhs);
        return true;
    }

    if top_level
        && ASSIGN_RIGHT.contains(&op.as_str())
        && rhs
            .as_ref()
            .is_some_and(|rhs| rhs.type_name() == "identifier")
        && lhs.is_some()
    {
        let (Some(rhs), Some(lhs)) = (rhs, lhs) else {
            return false;
        };
        let name = get_node_text(rhs, ctx.source());
        ctx.create_node(
            if is_constant_name(&name) {
                NodeKind::Constant
            } else {
                NodeKind::Variable
            },
            &name,
            node,
            NodeExtra::default(),
        );
        ctx.visit_node(lhs);
        return true;
    }

    false
}
