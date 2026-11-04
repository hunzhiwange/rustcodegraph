//! Native Solidity grammar registration; declarations are implemented separately.
use crate::extraction::tree_sitter_types::{ExtractorContext, LanguageExtractor};
use crate::web_tree_sitter::SyntaxNode;

pub struct SolidityExtractor;
pub const SOLIDITY_EXTRACTOR: SolidityExtractor = SolidityExtractor;
impl LanguageExtractor for SolidityExtractor {
    fn visit_node(&self, _node: &SyntaxNode, _ctx: &mut dyn ExtractorContext) -> bool {
        false
    }

    fn function_types(&self) -> &[&'static str] {
        &[]
    }
    fn class_types(&self) -> &[&'static str] {
        &[]
    }
    fn method_types(&self) -> &[&'static str] {
        &[]
    }
    fn interface_types(&self) -> &[&'static str] {
        &[]
    }
    fn struct_types(&self) -> &[&'static str] {
        &[]
    }
    fn enum_types(&self) -> &[&'static str] {
        &[]
    }
    fn type_alias_types(&self) -> &[&'static str] {
        &[]
    }
    fn import_types(&self) -> &[&'static str] {
        &[]
    }
    fn call_types(&self) -> &[&'static str] {
        &[]
    }
    fn variable_types(&self) -> &[&'static str] {
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
}
