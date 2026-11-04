//! Spring event listener registration evidence, based on upstream 15483713.
//! Registration creates no graph nodes or edges. Dispatch consumes typed targets
//! separately; annotation text in comments or adjacent declarations is not evidence.
use std::collections::HashMap;

use crate::{
    types::{Language, Node, NodeKind},
    web_tree_sitter::{Language as Grammar, Parser},
};

#[derive(Default)]
pub(super) struct ListenerRegistry {
    files: HashMap<String, Vec<(String, Node)>>,
}

impl ListenerRegistry {
    /// Replace per-file evidence, also removing stale registrations.
    pub(super) fn register(&mut self, file: &str, source: &str, nodes: &[Node]) {
        self.files.remove(file);
        if !file.ends_with(".java") {
            return;
        }
        let Ok(grammar) = Grammar::for_code_language(Language::Java) else {
            return;
        };
        let mut parser = Parser::default();
        parser.set_language(Some(grammar));
        let Some(tree) = parser.parse(source, None) else {
            return;
        };
        let mut entries = vec![];
        for method in tree.root_node.descendants_of_type(&["method_declaration"]) {
            if method.has_error || method.child_for_field_name("body").is_none() {
                continue;
            }
            let annotated =
                method
                    .named_children
                    .iter()
                    .filter(|n| n.node_type() == "modifiers")
                    .flat_map(|n| &n.named_children)
                    .filter(|n| matches!(n.node_type(), "annotation" | "marker_annotation"))
                    .any(|n| {
                        n.child_for_field_name("name").is_some_and(|name| {
                    matches!(name.text().as_str(), "EventListener" | "TransactionalEventListener"
                        | "org.springframework.context.event.EventListener"
                        | "org.springframework.transaction.event.TransactionalEventListener")
                })
                    });
            if !annotated {
                continue;
            }
            let Some(params) = method.child_for_field_name("parameters") else {
                continue;
            };
            let parameters: Vec<_> = params
                .named_children
                .iter()
                .filter(|n| {
                    matches!(
                        n.node_type(),
                        "formal_parameter" | "spread_parameter" | "receiver_parameter"
                    )
                })
                .collect();
            // Spring's parameter-typed listener has exactly one event parameter.
            if parameters.len() != 1 || parameters[0].node_type() != "formal_parameter" {
                continue;
            }
            let parameter = parameters[0];
            if parameter.child_for_field_name("dimensions").is_some() {
                continue;
            }
            let Some(event) = parameter.child_for_field_name("type") else {
                continue;
            };
            if !matches!(
                event.node_type(),
                "type_identifier" | "scoped_type_identifier"
            ) {
                continue;
            }
            let event: String = event.text().split_whitespace().collect();
            let Some(name) = method.child_for_field_name("name") else {
                continue;
            };
            let targets: Vec<_> = nodes
                .iter()
                .filter(|n| {
                    n.file_path == file
                        && n.language == Language::Java
                        && n.kind == NodeKind::Method
                        && n.name == name.text()
                        && n.start_line == method.start_position.row as u64 + 1
                        && n.start_column == method.start_position.column as u64
                })
                .collect();
            if targets.len() == 1 {
                entries.push((event, targets[0].clone()));
            }
        }
        self.files.insert(file.to_owned(), entries);
    }

    /// Preserve qualified names; dispatch owns application scope and type resolution.
    pub(super) fn targets(&self, event: &str) -> Vec<&Node> {
        let mut targets: Vec<_> = self
            .files
            .values()
            .flatten()
            .filter(|(name, _)| name == event)
            .map(|(_, target)| target)
            .collect();
        targets.sort_by(|a, b| a.id.cmp(&b.id));
        targets.dedup_by(|a, b| a.id == b.id);
        targets
    }
}
