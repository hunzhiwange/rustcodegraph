//! Celery registration evidence, based on upstream 15483713.
//!
//! Only actual Python decorator syntax registers a target. This stage creates
//! no graph nodes or edges; dispatch consumes the table in a separate pass.
//! Dispatch follows local/import bindings rather than upstream global-name fallback.
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
    #[allow(dead_code)] // Registration probe; dispatch requires stricter lexical binding.
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

/// AST dispatch sites use only local registrations or explicit Python imports.
/// Never fall back to a project-wide task name: separate apps may reuse names.
pub(super) fn dispatch_edges(
    ctx: &mut dyn crate::resolution::types::ResolutionContext,
) -> Vec<crate::types::Edge> {
    use crate::{
        types::{Edge, EdgeKind, EdgeProvenance},
        web_tree_sitter::SyntaxNode,
    };
    use serde_json::json;
    fn field<'a>(n: &'a SyntaxNode, name: &str) -> Option<&'a SyntaxNode> {
        n.child_for_field_name(name)
    }
    fn contains_name(n: &SyntaxNode, name: &str) -> bool {
        (n.node_type() == "identifier" && n.text() == name)
            || n.named_children.iter().any(|n| contains_name(n, name))
    }
    fn module_path(file: &str, source: &str) -> Option<String> {
        let dots = source.chars().take_while(|c| *c == '.').count();
        let mut parts: Vec<&str> = if dots > 0 {
            file.split('/').collect()
        } else {
            vec![]
        };
        if dots > 0 {
            for _ in 0..dots {
                parts.pop()?;
            }
        }
        parts.extend(source[dots..].split('.').filter(|s| !s.is_empty()));
        Some(parts.join("/"))
    }
    let Ok(grammar) = Grammar::for_code_language(Language::Python) else {
        return vec![];
    };
    let mut parser = Parser::default();
    parser.set_language(Some(grammar));
    let mut files = HashMap::new();
    let mut registry = TaskRegistry::default();
    for file in ctx
        .get_all_files()
        .into_iter()
        .filter(|f| f.ends_with(".py"))
    {
        let Some(source) = ctx.read_file(&file) else {
            continue;
        };
        let nodes = ctx.get_nodes_in_file(&file);
        registry.register(&file, &source, &nodes);
        if let Some(tree) = parser.parse(&source, None) {
            files.insert(file, (tree.root_node, nodes));
        }
    }
    let mut edges = vec![];
    for (file, (root, nodes)) in &files {
        // Only actual top-level imports establish bindings. Parse names through
        // AST fields so multiline imports, aliases, comments and strings are safe.
        let mut imports = Vec::new();
        for statement in &root.named_children {
            let namespace = statement.node_type() == "import_statement";
            if (!namespace && statement.node_type() != "import_from_statement")
                || statement.has_error
            {
                continue;
            }
            let module = field(statement, "module_name");
            for item in &statement.named_children {
                if module.is_some_and(|m| m.start_index == item.start_index) {
                    continue;
                }
                let (name, alias) = if item.node_type() == "aliased_import" {
                    let Some(name) = field(item, "name") else {
                        continue;
                    };
                    (name.text(), field(item, "alias").map(|n| n.text()))
                } else if matches!(item.node_type(), "dotted_name" | "identifier") {
                    (item.text(), None)
                } else {
                    continue;
                };
                // A dotted unaliased namespace needs a longer receiver chain;
                // leave that shape unconnected instead of inventing a binding.
                if namespace && alias.is_none() && name.contains('.') {
                    continue;
                }
                let local = alias.unwrap_or_else(|| name.clone());
                imports.push(crate::resolution::types::ImportMapping {
                    local_name: local,
                    exported_name: name.clone(),
                    source: if namespace {
                        name
                    } else {
                        let Some(m) = module else {
                            continue;
                        };
                        m.text()
                    },
                    is_default: false,
                    is_namespace: namespace,
                    resolved_path: None,
                });
            }
        }
        let mut seen = std::collections::HashSet::new();
        for call in root.descendants_of_type(&["call"]) {
            if seen.len() >= 80 || call.has_error {
                continue;
            }
            let Some(callee) = field(call, "function").filter(|n| n.node_type() == "attribute")
            else {
                continue;
            };
            if !field(callee, "attribute")
                .is_some_and(|n| matches!(n.text().as_str(), "delay" | "apply_async"))
            {
                continue;
            }
            let Some(receiver) = field(callee, "object") else {
                continue;
            };
            let (base, member) = match receiver.node_type() {
                "identifier" => (receiver.text(), None),
                "attribute" => {
                    let Some(object) =
                        field(receiver, "object").filter(|n| n.node_type() == "identifier")
                    else {
                        continue;
                    };
                    (
                        object.text(),
                        field(receiver, "attribute").map(|n| n.text()),
                    )
                }
                _ => continue,
            };
            // Reject rebinding and shadowing, including enclosing scopes. Broad
            // file-level assignment rejection is deliberately conservative.
            let shadowed = root
                .descendants_of_type(&["function_definition", "lambda"])
                .iter()
                .any(|n| {
                    n.start_index <= call.start_index
                        && n.end_index >= call.end_index
                        && field(n, "parameters").is_some_and(|p| contains_name(p, &base))
                })
                || root
                    .descendants_of_type(&[
                        "assignment",
                        "augmented_assignment",
                        "named_expression",
                        "for_statement",
                        "for_in_clause",
                    ])
                    .iter()
                    .any(|n| {
                        field(n, "left")
                            .or_else(|| field(n, "name"))
                            .is_some_and(|p| contains_name(p, &base))
                    });
            if shadowed {
                continue;
            }
            let bindings: Vec<_> = imports.iter().filter(|i| i.local_name == base).collect();
            let target = if bindings.len() == 1 {
                let binding = bindings[0];
                let Some(module) = module_path(file, &binding.source) else {
                    continue;
                };
                let paths = [format!("{module}.py"), format!("{module}/__init__.py")];
                let paths: Vec<_> = paths.iter().filter(|p| files.contains_key(*p)).collect();
                if paths.len() != 1 {
                    continue;
                }
                let name = if binding.is_namespace {
                    member.as_deref()
                } else if member.is_none() {
                    Some(binding.exported_name.as_str())
                } else {
                    None
                };
                name.and_then(|name| {
                    registry.files.get(paths[0]).and_then(|tasks| {
                        let matches: Vec<_> = tasks
                            .iter()
                            .filter(|n| n.name == name && n.start_column == 0)
                            .collect();
                        (matches.len() == 1).then(|| matches[0])
                    })
                })
            } else if bindings.is_empty() && member.is_none() {
                // A local target must be a module-level definition, not an
                // unrelated nested function in a different lexical scope.
                registry.files.get(file).and_then(|tasks| {
                    let matches: Vec<_> = tasks
                        .iter()
                        .filter(|n| n.name == base && n.start_column == 0)
                        .collect();
                    (matches.len() == 1).then(|| matches[0])
                })
            } else {
                None
            };
            let Some(target) = target else {
                continue;
            };
            let conflicting_definition = root
                .descendants_of_type(&["function_definition", "class_definition"])
                .iter()
                .any(|definition| {
                    field(definition, "name").is_some_and(|n| n.text() == base)
                        && !(target.file_path == *file
                            && definition.start_position.row as u64 + 1 == target.start_line
                            && definition.start_position.column == 0)
                });
            let local_import = root
                .descendants_of_type(&["import_statement", "import_from_statement"])
                .iter()
                .any(|import| {
                    !root
                        .named_children
                        .iter()
                        .any(|n| n.start_index == import.start_index)
                        && contains_name(import, &base)
                });
            if conflicting_definition || local_import {
                continue;
            }
            let line = call.start_position.row as u64 + 1;
            let Some(source) = nodes
                .iter()
                .filter(|n| {
                    matches!(n.kind, NodeKind::Function | NodeKind::Method)
                        && n.start_line <= line
                        && n.end_line >= line
                })
                .max_by_key(|n| n.start_line)
            else {
                continue;
            };
            if source.id == target.id || !seen.insert((source.id.clone(), target.id.clone())) {
                continue;
            }
            edges.push(Edge {
                source: source.id.clone(),
                target: target.id.clone(),
                kind: EdgeKind::Calls,
                line: Some(line),
                column: None,
                provenance: Some(EdgeProvenance::Heuristic),
                metadata: Some(HashMap::from([
                    ("synthesizedBy".into(), json!("celery-dispatch")),
                    ("via".into(), json!(receiver.text())),
                    ("registeredAt".into(), json!(format!("{file}:{line}"))),
                ])),
            });
        }
    }
    edges
}
