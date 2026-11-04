//! Native VB.NET parser wiring; semantic extraction is added separately.
use crate::extraction::tree_sitter_types::LanguageExtractor;
pub struct VbnetExtractor;
pub const VBNET_EXTRACTOR: VbnetExtractor = VbnetExtractor;
impl LanguageExtractor for VbnetExtractor {
    fn pre_parse(&self, source: &str) -> Option<String> {
        // Preserve all existing offsets while fixing the grammar's missing EOF terminator.
        (!source.ends_with('\n')).then(|| format!("{source}\n"))
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
