//! MediatR registration evidence, based on upstream 15483713.
//! Registration creates no nodes or edges. Dispatch consumes these typed targets
//! separately; a matching method name alone never registers a handler.
use std::collections::HashMap;

use crate::{
    types::{Language, Node, NodeKind},
    web_tree_sitter::{Language as Grammar, Parser, SyntaxNode},
};

// This prerequisite is consumed by the separate dispatch pass.
#[allow(dead_code)]
#[derive(Default)]
pub(super) struct HandlerRegistry {
    files: HashMap<String, Vec<(String, Node)>>,
}

fn type_name(node: &SyntaxNode) -> Option<String> {
    match node.node_type() {
        "identifier" | "qualified_name" | "alias_qualified_name" => {
            Some(node.text().split_whitespace().collect())
        }
        _ => None,
    }
}

#[allow(dead_code)]
impl HandlerRegistry {
    /// Replace per-file evidence, including deleted or invalid declarations.
    pub(super) fn register(&mut self, file: &str, source: &str, nodes: &[Node]) {
        self.files.remove(file);
        if !file.ends_with(".cs") {
            return;
        }
        let Ok(grammar) = Grammar::for_code_language(Language::CSharp) else {
            return;
        };
        let mut parser = Parser::default();
        parser.set_language(Some(grammar));
        let Some(tree) = parser.parse(source, None) else {
            return;
        };
        let mut entries = vec![];
        for class in tree.root_node.descendants_of_type(&["class_declaration"]) {
            if class.has_error {
                continue;
            }
            // Open handlers require runtime generic construction, not a concrete target.
            if class
                .named_children
                .iter()
                .any(|n| n.node_type() == "type_parameter_list")
            {
                continue;
            }
            let Some(bases) = class
                .named_children
                .iter()
                .find(|n| n.node_type() == "base_list")
            else {
                continue;
            };
            let mut requests = vec![];
            for base in &bases.named_children {
                let generic = if base.node_type() == "generic_name" {
                    Some(base)
                } else if base.node_type() == "qualified_name" {
                    base.named_children
                        .last()
                        .filter(|n| n.node_type() == "generic_name")
                } else {
                    None
                };
                let Some(generic) = generic else { continue };
                // The first generic argument is the request, never the response.
                // Custom handler interfaces use the same two-source type evidence.
                let Some(name) = generic.named_children.first() else {
                    continue;
                };
                if !name.text().starts_with('I') || !name.text().ends_with("Handler") {
                    continue;
                }
                if let Some(request) = generic
                    .named_children
                    .iter()
                    .find(|n| n.node_type() == "type_argument_list")
                    .and_then(|n| n.named_children.first())
                    .and_then(type_name)
                {
                    requests.push(request);
                }
            }
            let Some(body) = class.child_for_field_name("body") else {
                continue;
            };
            for method in body
                .named_children
                .iter()
                .filter(|n| n.node_type() == "method_declaration")
            {
                if !method
                    .child_for_field_name("name")
                    .is_some_and(|n| n.text() == "Handle")
                    || method.child_for_field_name("body").is_none()
                    || method.named_children.iter().any(|n| {
                        n.node_type() == "modifier"
                            && matches!(n.text().as_str(), "static" | "abstract")
                    })
                {
                    continue;
                }
                let Some(request) = method
                    .child_for_field_name("parameters")
                    .and_then(|n| {
                        n.named_children
                            .iter()
                            .find(|n| n.node_type() == "parameter")
                    })
                    .and_then(|n| n.child_for_field_name("type"))
                    .and_then(type_name)
                else {
                    continue;
                };
                if !requests.contains(&request) {
                    continue;
                }
                let targets: Vec<_> = nodes
                    .iter()
                    .filter(|n| {
                        n.file_path == file
                            && n.language == Language::CSharp
                            && n.kind == NodeKind::Method
                            && n.name == "Handle"
                            && n.start_line == method.start_position.row as u64 + 1
                            && n.start_column == method.start_position.column as u64
                    })
                    .collect();
                if targets.len() == 1 {
                    entries.push((request, targets[0].clone()));
                }
            }
        }
        self.files.insert(file.to_owned(), entries);
    }

    /// Exact type spellings only; dispatch owns solution scoping and ambiguity.
    pub(super) fn targets(&self, request: &str) -> Vec<&Node> {
        let mut targets: Vec<_> = self
            .files
            .values()
            .flatten()
            .filter(|(name, _)| name == request)
            .map(|(_, target)| target)
            .collect();
        targets.sort_by(|a, b| a.id.cmp(&b.id));
        targets.dedup_by(|a, b| a.id == b.id);
        targets
    }
}
