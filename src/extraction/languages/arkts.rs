//! ArkTS declaration and call shapes from upstream codegraph 15483713.
//! Reactive edge synthesis is handled separately.
use crate::extraction::tree_sitter_types::LanguageExtractor;
pub struct ArktsExtractor;
pub const ARKTS_EXTRACTOR: ArktsExtractor = ArktsExtractor;
impl LanguageExtractor for ArktsExtractor {
    fn function_types(&self) -> &[&'static str] {
        &["function_declaration"]
    }
    fn class_types(&self) -> &[&'static str] {
        &[]
    }
    fn method_types(&self) -> &[&'static str] {
        &["method_definition"]
    }
    fn interface_types(&self) -> &[&'static str] {
        &[]
    }
    fn struct_types(&self) -> &[&'static str] {
        &["struct_declaration"]
    }
    fn enum_types(&self) -> &[&'static str] {
        &[]
    }
    fn type_alias_types(&self) -> &[&'static str] {
        &[]
    }
    fn import_types(&self) -> &[&'static str] {
        &["import_statement"]
    }
    fn extract_import(
        &self,
        node: &crate::web_tree_sitter::SyntaxNode,
        source: &str,
    ) -> Option<crate::extraction::tree_sitter_types::ImportInfo> {
        super::typescript::TYPESCRIPT_EXTRACTOR.extract_import(node, source)
    }
    fn call_types(&self) -> &[&'static str] {
        &[
            "call_expression",
            "arkui_component_expression",
            "leading_dot_expression",
        ]
    }
    fn variable_types(&self) -> &[&'static str] {
        &[]
    }
    fn is_exported(&self, node: &crate::web_tree_sitter::SyntaxNode, source: &str) -> bool {
        super::typescript::TYPESCRIPT_EXTRACTOR.is_exported(node, source)
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
