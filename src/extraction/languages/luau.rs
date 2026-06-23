//! Rust translation of `src/extraction/languages/luau.ts`.

use crate::extraction::languages::lua::lua_extractor;
use crate::extraction::tree_sitter_helpers::{get_child_by_field, get_node_text};
use crate::extraction::tree_sitter_types::{ExtractorContext, LanguageExtractor, VariableInfo};
use crate::web_tree_sitter::SyntaxNode;

// Luau 先继承 Lua 的行为，再补类型定义、export 和返回类型签名，避免两套脚本语言规则漂移。
pub struct LuauExtractor;

pub const LUAU_EXTRACTOR: LuauExtractor = LuauExtractor;

pub fn luau_extractor() -> &'static LuauExtractor {
    &LUAU_EXTRACTOR
}

impl LanguageExtractor for LuauExtractor {
    fn function_types(&self) -> &'static [&'static str] {
        lua_extractor().function_types()
    }

    fn class_types(&self) -> &'static [&'static str] {
        lua_extractor().class_types()
    }

    fn method_types(&self) -> &'static [&'static str] {
        lua_extractor().method_types()
    }

    fn interface_types(&self) -> &'static [&'static str] {
        lua_extractor().interface_types()
    }

    fn struct_types(&self) -> &'static [&'static str] {
        lua_extractor().struct_types()
    }

    fn enum_types(&self) -> &'static [&'static str] {
        lua_extractor().enum_types()
    }

    fn type_alias_types(&self) -> &'static [&'static str] {
        &["type_definition"]
    }

    fn import_types(&self) -> &'static [&'static str] {
        lua_extractor().import_types()
    }

    fn call_types(&self) -> &'static [&'static str] {
        lua_extractor().call_types()
    }

    fn variable_types(&self) -> &'static [&'static str] {
        lua_extractor().variable_types()
    }

    fn name_field(&self) -> &'static str {
        lua_extractor().name_field()
    }

    fn body_field(&self) -> &'static str {
        lua_extractor().body_field()
    }

    fn params_field(&self) -> &'static str {
        lua_extractor().params_field()
    }

    fn is_exported(&self, node: &SyntaxNode, source: &str) -> bool {
        // tree-sitter-luau 没有稳定的 export 字段，直接看源码前缀最可靠。
        source.get(node.start_index()..node.start_index().saturating_add(7)) == Some("export ")
    }

    fn get_signature(&self, node: &SyntaxNode, source: &str) -> Option<String> {
        let params = get_child_by_field(node, "parameters")?;
        let mut sig = get_node_text(params, source);
        // Luau 的返回类型紧跟参数节点；遇到 block 说明没有显式类型。
        let kids = node.named_children();
        let idx = kids
            .iter()
            .position(|c| c.start_index() == params.start_index());
        let ret = idx.and_then(|i| kids.get(i + 1)).cloned();
        if ret.as_ref().is_some_and(|ret| ret.type_name() != "block") {
            sig.push_str(": ");
            sig.push_str(&get_node_text(ret.unwrap(), source));
        }
        Some(sig)
    }

    fn get_receiver_type(&self, node: &SyntaxNode, source: &str) -> Option<String> {
        lua_extractor().get_receiver_type(node, source)
    }

    fn visit_node(&self, node: &SyntaxNode, ctx: &mut dyn ExtractorContext) -> bool {
        lua_extractor().visit_node(node, ctx)
    }

    fn extract_variables(&self, node: &SyntaxNode, source: &str) -> Vec<VariableInfo> {
        lua_extractor().extract_variables(node, source)
    }
}
