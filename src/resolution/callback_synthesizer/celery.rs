//! Celery registration evidence, based on upstream 15483713.
//!
//! Only actual Python decorator syntax registers a target. This stage creates
//! no graph nodes or edges; dispatch consumes the table in a separate pass.
use crate::{
    types::{Language, Node, NodeKind},
    web_tree_sitter::{Language as Grammar, Parser},
};
use std::collections::HashMap;

#[derive(Default)]
pub(super) struct TaskRegistry {
    files: HashMap<String, Vec<Node>>,
}
impl TaskRegistry {
    /// Replace a file's evidence, so a removed decorator cannot remain registered.
    pub(super) fn register(&mut self, file: &str, source: &str, nodes: &[Node]) {
        self.files.remove(file);
        if !file.ends_with(".py") {
            return;
        }
        let Ok(grammar) = Grammar::for_code_language(Language::Python) else {
            return;
        };
        let mut parser = Parser::default();
        parser.set_language(Some(grammar));
        let Some(tree) = parser.parse(source, None) else {
            return;
        };
        let mut tasks = Vec::new();
        for decorated in tree
            .root_node
            .descendants_of_type(&["decorated_definition"])
        {
            let Some(function) = decorated.child_for_field_name("definition") else {
                continue;
            };
            if function.node_type() != "function_definition" || decorated.has_error {
                continue;
            }
            let registered = decorated
                .named_children
                .iter()
                .filter(|n| n.node_type() == "decorator")
                .any(|decorator| {
                    let Some(expr) = decorator.named_children.first() else {
                        return false;
                    };
                    let expr = if expr.node_type() == "call" {
                        let Some(callee) = expr.child_for_field_name("function") else {
                            return false;
                        };
                        callee
                    } else {
                        expr
                    };
                    // Accept bare @task/@shared_task and dotted app.task dialects,
                    // just as upstream does, without matching substrings or calls in text.
                    fn dotted(n: &crate::web_tree_sitter::SyntaxNode) -> bool {
                        n.node_type() == "identifier"
                            || (n.node_type() == "attribute"
                                && n.child_for_field_name("object").is_some_and(dotted))
                    }
                    if !dotted(expr) {
                        return false;
                    }
                    let name = if expr.node_type() == "identifier" {
                        Some(expr.text())
                    } else {
                        expr.child_for_field_name("attribute").map(|n| n.text())
                    };
                    matches!(name.as_deref(), Some("task" | "shared_task"))
                });
            if !registered {
                continue;
            }
            let Some(name) = function.child_for_field_name("name") else {
                continue;
            };
            let candidates: Vec<_> = nodes
                .iter()
                .filter(|n| {
                    n.kind == NodeKind::Function
                        && n.language == Language::Python
                        && n.file_path == file
                        && n.name == name.text()
                        && n.start_line == function.start_position.row as u64 + 1
                })
                .collect();
            if candidates.len() == 1 {
                tasks.push(candidates[0].clone());
            }
        }
        self.files.insert(file.to_owned(), tasks);
    }

    /// Prefer an unambiguous local registration, otherwise require a unique task.
    /// Import-aware dispatch can narrow candidates further; never guess among ties.
    pub(super) fn resolve(&self, name: &str, file: &str) -> Option<&Node> {
        let local: Vec<_> = self
            .files
            .get(file)
            .into_iter()
            .flatten()
            .filter(|n| n.name == name)
            .collect();
        if !local.is_empty() {
            return (local.len() == 1).then(|| local[0]);
        }
        let mut candidates = self.files.values().flatten().filter(|n| n.name == name);
        let first = candidates.next()?;
        candidates.next().is_none().then_some(first)
    }
}
