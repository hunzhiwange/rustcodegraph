//! Sidekiq registration evidence, based on upstream 15483713.
//! Parse direct class-body statements: nested classes, strings and singleton
//! methods must never supply another worker's registration or instance method.
//! This prerequisite creates no nodes or edges; dispatch consumes the table.
use crate::{
    types::{Language, Node, NodeKind},
    web_tree_sitter::{Language as Grammar, Parser},
};
use std::collections::HashMap;

#[derive(Default)]
#[allow(dead_code)] // Consumed by the separate Sidekiq dispatch stage.
pub(super) struct WorkerRegistry {
    files: HashMap<String, Vec<(Node, Node)>>,
}

#[allow(dead_code)]
impl WorkerRegistry {
    /// Replace evidence atomically per file, including removed registrations.
    pub(super) fn register(&mut self, file: &str, source: &str, nodes: &[Node]) {
        self.files.remove(file);
        if !file.ends_with(".rb") {
            return;
        }
        let Ok(grammar) = Grammar::for_code_language(Language::Ruby) else {
            return;
        };
        let mut parser = Parser::default();
        parser.set_language(Some(grammar));
        let Some(tree) = parser.parse(source, None) else {
            return;
        };
        let mut workers = vec![];
        for class in tree.root_node.descendants_of_type(&["class"]) {
            if class.has_error {
                continue;
            }
            let Some(body) = class.child_for_field_name("body") else {
                continue;
            };
            let registered = body.named_children.iter().any(|call| {
                call.node_type() == "call"
                    && call.child_for_field_name("receiver").is_none()
                    && call
                        .child_for_field_name("method")
                        .is_some_and(|n| n.text() == "include")
                    && call.child_for_field_name("arguments").is_some_and(|args| {
                        args.named_children.iter().any(|arg| {
                            arg.node_type() == "scope_resolution"
                                && matches!(
                                    arg.text().replace(' ', "").as_str(),
                                    "Sidekiq::Job"
                                        | "Sidekiq::Worker"
                                        | "::Sidekiq::Job"
                                        | "::Sidekiq::Worker"
                                )
                        })
                    })
            });
            if !registered {
                continue;
            }
            let classes: Vec<_> = nodes
                .iter()
                .filter(|n| {
                    n.file_path == file
                        && n.language == Language::Ruby
                        && n.kind == NodeKind::Class
                        && n.start_line == class.start_position.row as u64 + 1
                        && n.start_column == class.start_position.column as u64
                })
                .collect();
            if classes.len() != 1 {
                continue;
            }
            let methods: Vec<_> = body
                .named_children
                .iter()
                .filter(|n| {
                    n.node_type() == "method"
                        && n.child_for_field_name("name")
                            .is_some_and(|n| n.text() == "perform")
                })
                .collect();
            if methods.len() != 1 {
                continue;
            }
            let method = methods[0];
            let targets: Vec<_> = nodes
                .iter()
                .filter(|n| {
                    n.file_path == file
                        && n.language == Language::Ruby
                        && n.kind == NodeKind::Method
                        && n.name == "perform"
                        && n.start_line == method.start_position.row as u64 + 1
                        && n.start_column == method.start_position.column as u64
                })
                .collect();
            if targets.len() == 1 {
                workers.push((classes[0].clone(), targets[0].clone()));
            }
        }
        self.files.insert(file.to_owned(), workers);
    }

    /// Qualified references are exact; ambiguous short names produce no target.
    pub(super) fn resolve(&self, name: &str) -> Option<&Node> {
        let mut matches = self.files.values().flatten().filter(|(class, _)| {
            if name.contains("::") {
                class
                    .qualified_name
                    .strip_prefix(&format!("{}::", class.file_path))
                    .unwrap_or(&class.qualified_name)
                    == name.trim_start_matches("::")
            } else {
                class.name == name
            }
        });
        let (_, target) = matches.next()?;
        matches.next().is_none().then_some(target)
    }
}
