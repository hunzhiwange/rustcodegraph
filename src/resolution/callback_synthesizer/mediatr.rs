//! MediatR registration evidence, based on upstream 15483713.
//! Registration creates no nodes or edges. Dispatch consumes these typed targets
//! separately; a matching method name alone never registers a handler.
use std::collections::HashMap;

use crate::{
    types::{Language, Node, NodeKind},
    web_tree_sitter::{Language as Grammar, Parser, SyntaxNode},
};

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

/// Only literal construction is evidence of the dispatched request type.
/// Scope handlers by solution (or project when no solution exists), falling
/// back to the top-level application directory for loose source trees.
pub(super) fn dispatch_edges(
    ctx: &mut dyn crate::resolution::types::ResolutionContext,
) -> Vec<crate::types::Edge> {
    use crate::types::{Edge, EdgeKind, EdgeProvenance};
    use serde_json::json;
    fn scope(root: &std::path::Path, file: &str) -> String {
        let mut project = None;
        let mut dir = std::path::Path::new(file).parent();
        while let Some(path) = dir {
            let entries: Vec<_> = std::fs::read_dir(root.join(path))
                .into_iter()
                .flatten()
                .flatten()
                .map(|e| e.path())
                .collect();
            if entries
                .iter()
                .any(|p| p.extension().is_some_and(|e| e == "sln" || e == "slnx"))
            {
                return path.to_string_lossy().into_owned();
            }
            if project.is_none()
                && entries
                    .iter()
                    .any(|p| p.extension().is_some_and(|e| e == "csproj"))
            {
                project = Some(path.to_string_lossy().into_owned());
            }
            dir = path.parent();
        }
        project.unwrap_or_else(|| {
            file.split_once('/')
                .map_or("", |(first, _)| first)
                .to_owned()
        })
    }
    fn receiver_is_static(node: &SyntaxNode) -> bool {
        match node.node_type() {
            "identifier" | "this_expression" => true,
            "member_access_expression" => node
                .child_for_field_name("expression")
                .is_some_and(|n| receiver_is_static(&n)),
            _ => false,
        }
    }
    let Ok(grammar) = Grammar::for_code_language(Language::CSharp) else {
        return vec![];
    };
    let mut parser = Parser::default();
    parser.set_language(Some(grammar));
    let root = std::path::PathBuf::from(ctx.get_project_root());
    let mut registries: HashMap<String, HandlerRegistry> = HashMap::new();
    let mut files = vec![];
    let mut scopes = HashMap::new();
    for file in ctx
        .get_all_files()
        .into_iter()
        .filter(|f| f.ends_with(".cs"))
    {
        let Some(source) = ctx.read_file(&file) else {
            continue;
        };
        let nodes = ctx.get_nodes_in_file(&file);
        let directory = std::path::Path::new(&file)
            .parent()
            .unwrap_or(std::path::Path::new(""))
            .to_owned();
        let scope = scopes
            .entry(directory)
            .or_insert_with(|| scope(&root, &file))
            .clone();
        registries
            .entry(scope.clone())
            .or_default()
            .register(&file, &source, &nodes);
        if let Some(tree) = parser.parse(&source, None) {
            files.push((file, scope, tree.root_node, nodes));
        }
    }
    let mut edges = vec![];
    for (file, scope, root, nodes) in files {
        let mut seen = std::collections::HashSet::new();
        for call in root.descendants_of_type(&["invocation_expression"]) {
            if call.has_error || seen.len() >= 80 {
                continue;
            }
            let Some(function) = call.child_for_field_name("function") else {
                continue;
            };
            if function.node_type() != "member_access_expression" {
                continue;
            }
            let Some(verb) = function.child_for_field_name("name") else {
                continue;
            };
            if !matches!(verb.text().as_str(), "Send" | "Publish") {
                continue;
            }
            let Some(receiver) = function.child_for_field_name("expression") else {
                continue;
            };
            // A simple mediator-ish field/identifier, never a computed receiver.
            if !receiver_is_static(&receiver) {
                continue;
            }
            let receiver_text = receiver.text().to_ascii_lowercase();
            if !["mediator", "sender", "publisher"]
                .iter()
                .any(|name| receiver_text.contains(name))
            {
                continue;
            }
            let Some(argument) = call
                .child_for_field_name("arguments")
                .and_then(|args| args.named_children.first())
                .and_then(|arg| arg.named_children.first())
            else {
                continue;
            };
            if argument.node_type() != "object_creation_expression" {
                continue;
            }
            let Some(request) = argument.child_for_field_name("type").and_then(type_name) else {
                continue;
            };
            let targets = registries[&scope].targets(&request);
            // Send is single-target: ambiguous requests cannot be resolved statically.
            if verb.text() == "Send" && targets.len() != 1 {
                continue;
            }
            let line = call.start_position.row as u64 + 1;
            let Some(source) = nodes
                .iter()
                .filter(|n| {
                    matches!(n.kind, NodeKind::Method | NodeKind::Function)
                        && n.start_line <= line
                        && n.end_line >= line
                        && (n.start_line < line
                            || n.start_column <= call.start_position.column as u64)
                })
                .max_by_key(|n| (n.start_line, n.start_column))
            else {
                continue;
            };
            for target in targets {
                if seen.len() >= 80 {
                    break;
                }
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
                        ("synthesizedBy".into(), json!("mediatr-dispatch")),
                        ("via".into(), json!(request)),
                        ("registeredAt".into(), json!(format!("{file}:{line}"))),
                    ])),
                });
            }
        }
    }
    edges
}
