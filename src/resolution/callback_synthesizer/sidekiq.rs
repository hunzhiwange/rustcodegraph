//! Sidekiq registration evidence, based on upstream 15483713.
//! Parse direct class-body statements: nested classes, strings and singleton
//! methods must never supply another worker's registration or instance method.
//! Dispatch consumes registration evidence without creating additional nodes.
use crate::{
    types::{Language, Node, NodeKind},
    web_tree_sitter::{Language as Grammar, Parser},
};
use std::collections::HashMap;

#[derive(Default)]
pub(super) struct WorkerRegistry {
    files: HashMap<String, Vec<(Node, Node)>>,
}

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
                    .split_once("::")
                    .map_or(class.qualified_name.as_str(), |(_, name)| name)
                    == name.trim_start_matches("::")
            } else {
                class.name == name
            }
        });
        let (_, target) = matches.next()?;
        matches.next().is_none().then_some(target)
    }
}

/// Literal constant receivers only, scoped to the same application tree.
/// Rails' app/ layout shares a root; standalone Ruby files share their directory.
pub(super) fn dispatch_edges(
    ctx: &mut dyn crate::resolution::types::ResolutionContext,
) -> Vec<crate::types::Edge> {
    use crate::types::{Edge, EdgeKind, EdgeProvenance};
    use crate::web_tree_sitter::SyntaxNode;
    use serde_json::json;
    fn app_root(file: &str) -> &str {
        if file.starts_with("app/") {
            ""
        } else if let Some((root, _)) = file.split_once("/app/") {
            root
        } else {
            file.rsplit_once('/').map_or("", |(dir, _)| dir)
        }
    }
    fn constant(n: &SyntaxNode) -> bool {
        n.node_type() == "constant"
            || (n.node_type() == "scope_resolution" && n.named_children.iter().all(constant))
    }
    let Ok(grammar) = Grammar::for_code_language(Language::Ruby) else {
        return vec![];
    };
    let mut parser = Parser::default();
    parser.set_language(Some(grammar));
    let mut files = Vec::new();
    let mut registries: HashMap<String, WorkerRegistry> = HashMap::new();
    for file in ctx
        .get_all_files()
        .into_iter()
        .filter(|f| f.ends_with(".rb"))
    {
        let Some(source) = ctx.read_file(&file) else {
            continue;
        };
        let nodes = ctx.get_nodes_in_file(&file);
        registries
            .entry(app_root(&file).to_owned())
            .or_default()
            .register(&file, &source, &nodes);
        if let Some(tree) = parser.parse(&source, None) {
            files.push((file, tree.root_node, nodes));
        }
    }
    let mut edges = vec![];
    for (file, root, nodes) in files {
        let registry = &registries[app_root(&file)];
        let mut seen = std::collections::HashSet::new();
        for call in root.descendants_of_type(&["call"]) {
            if call.has_error || seen.len() >= 80 {
                continue;
            }
            if !call.child_for_field_name("method").is_some_and(|n| {
                matches!(
                    n.text().as_str(),
                    "perform_async" | "perform_in" | "perform_at"
                )
            }) {
                continue;
            }
            let Some(receiver) = call
                .child_for_field_name("receiver")
                .filter(|n| constant(n))
            else {
                continue;
            };
            let name = receiver.text().replace(' ', "");
            // Constant assignments can rebind a worker; do not guess through aliases.
            if root.descendants_of_type(&["assignment"]).iter().any(|n| {
                n.child_for_field_name("left")
                    .is_some_and(|left| left.text() == name)
            }) {
                continue;
            }
            let Some(target) = registry.resolve(&name) else {
                continue;
            };
            // Nested lexical namespaces may shadow relative constant paths.
            // Require an absolute receiver until lexical lookup is proven.
            if !name.starts_with("::")
                && root
                    .descendants_of_type(&["module", "class"])
                    .iter()
                    .any(|n| n.start_index <= call.start_index && n.end_index >= call.end_index)
            {
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
                .max_by_key(|n| (n.start_line, n.start_column))
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
                column: Some(call.start_position.column as u64),
                provenance: Some(EdgeProvenance::Heuristic),
                metadata: Some(HashMap::from([
                    ("synthesizedBy".into(), json!("sidekiq-dispatch")),
                    ("via".into(), json!(name)),
                    ("registeredAt".into(), json!(format!("{file}:{line}"))),
                ])),
            });
        }
    }
    edges
}
