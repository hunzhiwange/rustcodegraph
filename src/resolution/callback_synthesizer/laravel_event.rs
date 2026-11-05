//! Laravel registration evidence from upstream 15483713. Dispatch is a separate pass.
use crate::{
    types::{Language, Node, NodeKind},
    web_tree_sitter::{Language as Grammar, Parser, SyntaxNode},
};
use std::collections::HashMap;

#[derive(Default)]
#[allow(dead_code)] // Consumed by the subsequent Laravel dispatch task.
pub(super) struct ListenerRegistry {
    files: HashMap<String, Evidence>,
}
#[derive(Default)]
struct Evidence {
    typed: Vec<(String, Node)>,
    handles: Vec<(String, Node)>,
    mappings: Vec<(String, String)>,
}
fn class_reference(n: &SyntaxNode) -> Option<String> {
    match n.node_type() {
        "class_constant_access_expression"
            if n.named_children.len() == 2
                && n.named_children[1].text().eq_ignore_ascii_case("class")
                && matches!(n.named_children[0].node_type(), "name" | "qualified_name") =>
        {
            Some(n.named_children[0].text())
        }
        "string" => {
            let text = n.text();
            let value = text.get(1..text.len().checked_sub(1)?)?;
            if !value.is_empty()
                && value
                    .chars()
                    .all(|c| c.is_alphanumeric() || c == '_' || c == '\\')
            {
                Some(value.to_owned())
            } else {
                None
            }
        }
        _ => None,
    }
}
#[allow(dead_code)]
impl ListenerRegistry {
    /// Replace a file's evidence, including removal of stale registrations.
    pub(super) fn register(&mut self, file: &str, source: &str, nodes: &[Node]) {
        self.files.remove(file);
        if !file.ends_with(".php") {
            return;
        }
        let Ok(grammar) = Grammar::for_code_language(Language::Php) else {
            return;
        };
        let mut parser = Parser::default();
        parser.set_language(Some(grammar));
        let Some(tree) = parser.parse(source, None) else {
            return;
        };
        let mut evidence = Evidence::default();
        for class in tree.root_node.descendants_of_type(&["class_declaration"]) {
            let (Some(name), Some(body)) = (
                class.child_for_field_name("name"),
                class.child_for_field_name("body"),
            ) else {
                continue;
            };
            for method in body
                .named_children
                .iter()
                .filter(|n| n.node_type() == "method_declaration")
            {
                if method.has_error
                    || method.child_for_field_name("body").is_none()
                    || !method
                        .child_for_field_name("name")
                        .is_some_and(|n| n.text() == "handle")
                {
                    continue;
                }
                let targets: Vec<_> = nodes
                    .iter()
                    .filter(|n| {
                        n.file_path == file
                            && n.language == Language::Php
                            && n.kind == NodeKind::Method
                            && n.name == "handle"
                            && n.start_line == method.start_position.row as u64 + 1
                            && n.start_column == method.start_position.column as u64
                    })
                    .collect();
                if targets.len() != 1 {
                    continue;
                }
                let target = targets[0];
                evidence.handles.push((name.text(), target.clone()));
                let Some(parameters) = method.child_for_field_name("parameters") else {
                    continue;
                };
                let Some(first) = parameters
                    .named_children
                    .iter()
                    .find(|n| n.node_type() != "comment")
                else {
                    continue;
                };
                let Some(ty) = first.child_for_field_name("type") else {
                    continue;
                };
                let types = if ty.node_type() == "named_type" {
                    vec![ty]
                } else if matches!(ty.node_type(), "union_type" | "optional_type") {
                    ty.named_children
                        .iter()
                        .filter(|n| n.node_type() == "named_type")
                        .collect()
                } else {
                    vec![]
                };
                for ty in types {
                    evidence.typed.push((ty.text(), target.clone()));
                }
            }
            for property in body
                .named_children
                .iter()
                .filter(|n| n.node_type() == "property_declaration" && !n.has_error)
            {
                for element in property
                    .named_children
                    .iter()
                    .filter(|n| n.node_type() == "property_element")
                {
                    if !element
                        .child_for_field_name("name")
                        .is_some_and(|n| n.text() == "$listen")
                    {
                        continue;
                    }
                    let Some(array) = element
                        .child_for_field_name("default_value")
                        .filter(|n| n.node_type() == "array_creation_expression")
                    else {
                        continue;
                    };
                    for entry in array
                        .named_children
                        .iter()
                        .filter(|n| n.node_type() == "array_element_initializer")
                    {
                        if entry.named_children.len() != 2 {
                            continue;
                        }
                        let Some(event) = class_reference(&entry.named_children[0]) else {
                            continue;
                        };
                        let listeners = &entry.named_children[1];
                        if listeners.node_type() != "array_creation_expression" {
                            continue;
                        }
                        for listener in listeners.named_children.iter().filter(|n| {
                            n.node_type() == "array_element_initializer"
                                && n.named_children.len() == 1
                        }) {
                            if let Some(name) = class_reference(&listener.named_children[0]) {
                                evidence.mappings.push((event.clone(), name));
                            }
                        }
                    }
                }
            }
        }
        self.files.insert(file.to_owned(), evidence);
    }
    /// Exact type references only; dispatch must resolve namespace/import and app scope.
    pub(super) fn targets(&self, event: &str) -> Vec<&Node> {
        let mut targets: Vec<_> = self
            .files
            .values()
            .flat_map(|f| &f.typed)
            .filter(|(e, _)| e == event)
            .map(|(_, n)| n)
            .collect();
        for (_, listener) in self
            .files
            .values()
            .flat_map(|f| &f.mappings)
            .filter(|(e, _)| e == event)
        {
            let candidates: Vec<_> = self
                .files
                .values()
                .flat_map(|f| &f.handles)
                .filter(|(name, _)| name == listener)
                .map(|(_, n)| n)
                .collect();
            if candidates.len() == 1 {
                targets.push(candidates[0]);
            }
        }
        targets.sort_by(|a, b| a.id.cmp(&b.id));
        targets.dedup_by(|a, b| a.id == b.id);
        targets
    }
}
