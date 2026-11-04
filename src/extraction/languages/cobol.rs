//! Native COBOL parser entry; semantic extraction is implemented separately.
use crate::extraction::tree_sitter_types::LanguageExtractor;
pub struct CobolExtractor;
pub const COBOL_EXTRACTOR: CobolExtractor = CobolExtractor;
impl LanguageExtractor for CobolExtractor {
    fn pre_parse(&self, source: &str) -> Option<String> {
        static MARKER: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
            regex::Regex::new(r"(?i)^([ \t]*)(IDENTIFICATION\s+DIVISION|ID\s+DIVISION|PROGRAM-ID\b|\d{2}[ \t]+[A-Za-z])").unwrap()
        });
        let free = source
            .lines()
            .find_map(|line| MARKER.captures(line).map(|m| m[1].len() < 7))
            .unwrap_or(false);
        if !free {
            return None;
        }
        // Preserve lines; enable the patched scanner's wide free-format margin.
        Some(
            source
                .split('\n')
                .enumerate()
                .map(|(i, line)| {
                    if i == 0 {
                        format!("CGWIDE {line}")
                    } else if line.is_empty() {
                        String::new()
                    } else {
                        format!("       {line}")
                    }
                })
                .collect::<Vec<_>>()
                .join("\n"),
        )
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
