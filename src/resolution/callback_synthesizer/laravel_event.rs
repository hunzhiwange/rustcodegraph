//! Laravel registration and scoped literal event dispatch, based on upstream 15483713.
use crate::{
    types::{Language, Node, NodeKind},
    web_tree_sitter::{Language as Grammar, Parser, SyntaxNode},
};
use std::collections::HashMap;

#[derive(Default)]
pub(super) struct ListenerRegistry {
    files: HashMap<String, Evidence>,
}
#[derive(Default)]
struct Evidence {
    typed: Vec<(String, Node)>,
    handles: Vec<(String, Node)>,
    mappings: Vec<(String, String, u64)>,
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
                                evidence.mappings.push((
                                    event.clone(),
                                    name,
                                    property.start_position.row as u64 + 1,
                                ));
                            }
                        }
                    }
                }
            }
        }
        self.files.insert(file.to_owned(), evidence);
    }
    /// Exact type references only; dispatch must resolve namespace/import and app scope.
    #[allow(dead_code)] // Registration contract tests also exercise raw references.
    pub(super) fn targets(&self, event: &str) -> Vec<&Node> {
        let mut targets: Vec<_> = self
            .files
            .values()
            .flat_map(|f| &f.typed)
            .filter(|(e, _)| e == event)
            .map(|(_, n)| n)
            .collect();
        for (_, listener, _) in self
            .files
            .values()
            .flat_map(|f| &f.mappings)
            .filter(|(e, _, _)| e == event)
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

/// PHP namespace/import context at a declaration or dispatch position.
fn qualify(root: &SyntaxNode, line: u64, name: &str) -> Option<String> {
    if name.starts_with('\\') {
        return Some(name.trim_start_matches('\\').to_ascii_lowercase());
    }
    let namespace = root
        .descendants_of_type(&["namespace_definition"])
        .into_iter()
        .filter(|n| {
            (n.start_position.row as u64) < line
                && (n.child_for_field_name("body").is_none()
                    || n.end_position.row as u64 + 1 >= line)
        })
        .max_by_key(|n| n.start_position.row);
    let ns = namespace
        .and_then(|n| n.child_for_field_name("name"))
        .map(|n| n.text())
        .unwrap_or_default();
    let container = namespace
        .and_then(|n| n.child_for_field_name("body"))
        .unwrap_or(root);
    let mut aliases = HashMap::new();
    for declaration in &container.named_children {
        if declaration.node_type() != "namespace_use_declaration" {
            continue;
        }
        if container.id == root.id
            && namespace.is_some_and(|ns| {
                declaration.start_index < ns.end_index
                    || root.named_children.iter().any(|next| {
                        next.node_type() == "namespace_definition"
                            && next.start_index > ns.start_index
                            && next.start_index < declaration.start_index
                    })
            })
        {
            continue;
        }
        // Grouped imports are deliberately unresolved until their prefix is proven.
        if declaration
            .named_children
            .iter()
            .any(|n| n.node_type() == "namespace_use_group")
        {
            return None;
        }
        let text = declaration.text();
        if text.starts_with("use function ") || text.starts_with("use const ") {
            continue;
        }
        for clause in declaration
            .named_children
            .iter()
            .filter(|n| n.node_type() == "namespace_use_clause")
        {
            let words: Vec<_> = clause
                .text()
                .split_whitespace()
                .map(str::to_owned)
                .collect();
            let Some(path) = words.first() else {
                continue;
            };
            let alias = if words.len() == 3 && words[1].eq_ignore_ascii_case("as") {
                &words[2]
            } else {
                path.rsplit('\\').next().unwrap_or(path)
            };
            if aliases
                .insert(
                    alias.to_ascii_lowercase(),
                    path.trim_start_matches('\\').to_owned(),
                )
                .is_some()
            {
                return None;
            }
        }
    }
    let (first, rest) = name.split_once('\\').unwrap_or((name, ""));
    let qualified = if let Some(import) = aliases.get(&first.to_ascii_lowercase()) {
        if rest.is_empty() {
            import.clone()
        } else {
            format!("{import}\\{rest}")
        }
    } else if let Some(relative) = name.strip_prefix("namespace\\") {
        format!("{ns}\\{relative}")
    } else if ns.is_empty() {
        name.to_owned()
    } else {
        format!("{ns}\\{name}")
    };
    Some(qualified.to_ascii_lowercase())
}

fn scope(project: &std::path::Path, file: &str) -> String {
    let mut directory = std::path::Path::new(file).parent();
    while let Some(path) = directory {
        if project.join(path).join("composer.json").is_file() {
            return path.to_string_lossy().into_owned();
        }
        directory = path.parent();
    }
    file.split_once('/')
        .map_or("", |(first, _)| first)
        .to_owned()
}

pub(super) fn dispatch_edges(
    ctx: &mut dyn crate::resolution::types::ResolutionContext,
) -> Vec<crate::types::Edge> {
    use crate::types::{Edge, EdgeKind, EdgeProvenance};
    use serde_json::json;
    let Ok(grammar) = Grammar::for_code_language(Language::Php) else {
        return vec![];
    };
    let mut parser = Parser::default();
    parser.set_language(Some(grammar));
    let project = std::path::PathBuf::from(ctx.get_project_root());
    let mut files = vec![];
    let mut handles: HashMap<(String, String), Vec<Node>> = HashMap::new();
    let mut listeners: HashMap<(String, String), Vec<Node>> = HashMap::new();
    let mut mappings = vec![];
    for file in ctx
        .get_all_files()
        .into_iter()
        .filter(|f| f.ends_with(".php"))
    {
        let Some(source) = ctx.read_file(&file) else {
            continue;
        };
        let Some(tree) = parser.parse(&source, None) else {
            continue;
        };
        let nodes = ctx.get_nodes_in_file(&file);
        let app = scope(&project, &file);
        let mut registry = ListenerRegistry::default();
        registry.register(&file, &source, &nodes);
        for evidence in registry.files.values() {
            for (name, target) in &evidence.handles {
                if let Some(name) = qualify(&tree.root_node, target.start_line, name) {
                    handles
                        .entry((app.clone(), name))
                        .or_default()
                        .push(target.clone());
                }
            }
            for (event, target) in &evidence.typed {
                if let Some(event) = qualify(&tree.root_node, target.start_line, event) {
                    listeners
                        .entry((app.clone(), event))
                        .or_default()
                        .push(target.clone());
                }
            }
            for (event, listener, line) in &evidence.mappings {
                if let (Some(event), Some(listener)) = (
                    qualify(&tree.root_node, *line, event),
                    qualify(&tree.root_node, *line, listener),
                ) {
                    mappings.push((app.clone(), event, listener));
                }
            }
        }
        files.push((file, app, tree.root_node, nodes));
    }
    for (app, event, listener) in mappings {
        if let Some(targets) = handles.get(&(app.clone(), listener))
            && targets.len() == 1
        {
            listeners
                .entry((app, event))
                .or_default()
                .push(targets[0].clone());
        }
    }
    for targets in listeners.values_mut() {
        targets.sort_by(|a, b| a.id.cmp(&b.id));
        targets.dedup_by(|a, b| a.id == b.id);
    }
    let mut edges = vec![];
    for (file, app, root, nodes) in files {
        let mut seen = std::collections::HashSet::new();
        for call in root.descendants_of_type(&["function_call_expression"]) {
            if call.has_error || seen.len() >= 80 {
                continue;
            }
            if !call
                .child_for_field_name("function")
                .is_some_and(|n| matches!(n.text().as_str(), "event" | "\\event"))
            {
                continue;
            }
            let Some(arguments) = call.child_for_field_name("arguments") else {
                continue;
            };
            if arguments.named_children.len() != 1 {
                continue;
            }
            let argument = &arguments.named_children[0];
            let Some(creation) = argument
                .named_children
                .first()
                .filter(|n| n.node_type() == "object_creation_expression")
            else {
                continue;
            };
            let Some(ty) = creation
                .named_children
                .first()
                .filter(|n| matches!(n.node_type(), "name" | "qualified_name"))
            else {
                continue;
            };
            let line = call.start_position.row as u64 + 1;
            let Some(event) = qualify(&root, line, &ty.text()) else {
                continue;
            };
            let Some(targets) = listeners.get(&(app.clone(), event.clone())) else {
                continue;
            };
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
                        ("synthesizedBy".into(), json!("laravel-event")),
                        ("via".into(), json!(event)),
                        ("registeredAt".into(), json!(format!("{file}:{line}"))),
                    ])),
                });
            }
        }
    }
    edges
}
