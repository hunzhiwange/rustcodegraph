//! Batch-owned conservative lexical binding guards for JS/TS.
use super::{ReferenceResolver, ResolutionContext, UnresolvedRef};
use crate::types::{Language, ReferenceKind};
#[derive(Default)]
pub(super) struct Bindings {
    pub source: String,
    pub sites: Vec<(String, usize, usize, bool)>,
}
fn names(n: tree_sitter::Node<'_>, source: &[u8], out: &mut Vec<String>) {
    if matches!(
        n.kind(),
        "identifier" | "shorthand_property_identifier_pattern"
    ) {
        if let Ok(s) = n.utf8_text(source) {
            out.push(s.to_owned());
        }
        return;
    }
    if matches!(
        n.kind(),
        "assignment_pattern" | "pair_pattern" | "required_parameter" | "optional_parameter"
    ) {
        if let Some(c) = n
            .child_by_field_name("left")
            .or_else(|| n.child_by_field_name("value"))
            .or_else(|| n.child_by_field_name("pattern"))
        {
            names(c, source, out);
        }
        return;
    }
    let mut c = n.walk();
    for child in n.named_children(&mut c) {
        names(child, source, out);
    }
}
fn walk(
    n: tree_sitter::Node<'_>,
    source: &[u8],
    scope: (usize, usize),
    sites: &mut Vec<(String, usize, usize, bool)>,
) {
    if matches!(n.kind(), "function_declaration" | "class_declaration") {
        if let Some(name) = n
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(source).ok())
        {
            sites.push((name.to_owned(), scope.0, scope.1, false));
        }
    }
    let scope = if matches!(
        n.kind(),
        "statement_block"
            | "function_declaration"
            | "function_expression"
            | "arrow_function"
            | "method_definition"
            | "catch_clause"
            | "for_statement"
            | "for_in_statement"
    ) {
        (n.start_byte(), n.end_byte())
    } else {
        scope
    };
    if n.kind() == "variable_declarator" {
        let mut binding_scope = scope;
        if n.parent()
            .is_some_and(|p| p.kind() == "variable_declaration")
        {
            let mut parent = n.parent();
            while let Some(p) = parent {
                if matches!(
                    p.kind(),
                    "function_declaration"
                        | "function_expression"
                        | "arrow_function"
                        | "method_definition"
                        | "program"
                ) {
                    binding_scope = (p.start_byte(), p.end_byte());
                    break;
                }
                parent = p.parent();
            }
        }
        let known = n.child_by_field_name("value").is_some_and(|v| {
            matches!(
                v.kind(),
                "arrow_function" | "function_expression" | "new_expression"
            )
        });
        // Unknown local values shadow global/imported names. Do not infer their targets.
        if let Some(p) = n.child_by_field_name("name") {
            let mut ns = Vec::new();
            names(p, source, &mut ns);
            for name in ns {
                sites.push((name, binding_scope.0, binding_scope.1, !known));
            }
        }
    } else if n.kind() == "formal_parameters" {
        let mut ns = Vec::new();
        names(n, source, &mut ns);
        for name in ns {
            sites.push((name, scope.0, scope.1, true));
        }
    }
    if n.kind() == "arrow_function" {
        if let Some(p) = n.child_by_field_name("parameter") {
            let mut ns = Vec::new();
            names(p, source, &mut ns);
            for name in ns {
                sites.push((name, scope.0, scope.1, true));
            }
        }
    }
    if n.kind() == "catch_clause" {
        if let Some(p) = n.child_by_field_name("parameter") {
            let mut ns = Vec::new();
            names(p, source, &mut ns);
            for name in ns {
                sites.push((name, scope.0, scope.1, true));
            }
        }
    }
    let mut c = n.walk();
    for child in n.named_children(&mut c) {
        walk(child, source, scope, sites);
    }
}
impl ReferenceResolver<'_> {
    pub fn js_binding_analysis_count(&self) -> usize {
        self.js_analysis_count
    }
    pub fn js_local_binding_blocks(&mut self, r: &UnresolvedRef) -> bool {
        if !matches!(
            r.language,
            Language::JavaScript | Language::Jsx | Language::TypeScript | Language::Tsx
        ) || !matches!(
            r.reference_kind,
            ReferenceKind::Calls | ReferenceKind::FunctionRef
        ) {
            return false;
        }
        let key = format!("{:?}:{}", r.language, r.file_path);
        if !self.js_bindings.contains_key(&key) {
            let Some(source) = self.read_file(&r.file_path) else {
                return false;
            };
            let lang = match r.language {
                Language::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
                Language::Tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
                _ => tree_sitter_javascript::LANGUAGE.into(),
            };
            let mut parser = tree_sitter::Parser::new();
            if parser.set_language(&lang).is_err() {
                return false;
            }
            let Some(tree) = parser.parse(&source, None) else {
                return false;
            };
            let mut sites = Vec::new();
            walk(
                tree.root_node(),
                source.as_bytes(),
                (0, source.len()),
                &mut sites,
            );
            // Bound retained source snapshots in large repositories. Eviction only
            // recomputes deterministic analysis; it cannot change the result.
            if self.js_bindings.len() >= 64 {
                self.js_bindings.clear();
            }
            self.js_analysis_count += 1;
            self.js_bindings
                .insert(key.clone(), Bindings { source, sites });
        }
        let b = &self.js_bindings[&key];
        let start = b
            .source
            .split_inclusive('\n')
            .take(r.line.saturating_sub(1) as usize)
            .map(str::len)
            .sum::<usize>();
        let at = start.saturating_add(r.column as usize);
        let receiver = r.reference_name.split('.').next().unwrap_or("");
        b.sites
            .iter()
            .filter(|(name, start, end, _)| name == receiver && *start <= at && at < *end)
            .min_by_key(|(_, start, end, _)| end - start)
            .is_some_and(|(_, _, _, blocks)| *blocks)
    }
}
