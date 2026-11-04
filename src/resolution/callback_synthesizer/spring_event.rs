//! Spring event listener registration evidence, based on upstream 15483713.
//! Registration creates no graph nodes or edges. Dispatch consumes typed targets;
//! annotation text in comments or adjacent declarations is not evidence.
use std::collections::HashMap;

use crate::{
    types::{Language, Node, NodeKind},
    web_tree_sitter::{Language as Grammar, Parser, SyntaxNode},
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

/// Resolve only explicit imports, qualified names and the current package.
/// Wildcard imports require a classpath and therefore remain unresolved.
fn event_type(root: &SyntaxNode, name: &str) -> Option<String> {
    if name.contains('.') {
        return Some(name.to_owned());
    }
    let mut package = String::new();
    let mut imports = Vec::new();
    let mut wildcard = false;
    for declaration in &root.named_children {
        match declaration.node_type() {
            "package_declaration" => {
                if let Some(path) = declaration
                    .named_children
                    .iter()
                    .find(|n| matches!(n.node_type(), "identifier" | "scoped_identifier"))
                {
                    package = path.text();
                }
            }
            "import_declaration" => {
                let text: String = declaration.text().split_whitespace().collect();
                if text.starts_with("importstatic") {
                    continue;
                }
                wildcard |= text.contains('*');
                if let Some(path) = declaration
                    .named_children
                    .iter()
                    .find(|n| matches!(n.node_type(), "identifier" | "scoped_identifier"))
                {
                    let path = path.text();
                    if path.rsplit('.').next() == Some(name) {
                        imports.push(path);
                    }
                }
            }
            _ => {}
        }
    }
    imports.sort();
    imports.dedup();
    match imports.len() {
        1 => imports.pop(),
        0 if !wildcard => Some(if package.is_empty() {
            name.to_owned()
        } else {
            format!("{package}.{name}")
        }),
        _ => None,
    }
}

fn application_scope(root: &std::path::Path, file: &str) -> String {
    let mut directory = std::path::Path::new(file).parent();
    while let Some(path) = directory {
        if [
            "pom.xml",
            "build.gradle",
            "build.gradle.kts",
            "settings.gradle",
            "settings.gradle.kts",
        ]
        .iter()
        .any(|marker| root.join(path).join(marker).is_file())
        {
            return path.to_string_lossy().into_owned();
        }
        directory = path.parent();
    }
    file.split_once('/')
        .map_or("", |(first, _)| first)
        .to_owned()
}

fn static_receiver(node: &SyntaxNode) -> bool {
    match node.node_type() {
        "identifier" | "this" => true,
        "field_access" => node
            .child_for_field_name("object")
            .is_some_and(|n| static_receiver(&n)),
        _ => false,
    }
}

/// Bridge literal publishEvent construction to registered, package-qualified listeners.
/// Rebuild evidence every resolution pass so removed annotations cannot leave stale edges.
pub(super) fn dispatch_edges(
    ctx: &mut dyn crate::resolution::types::ResolutionContext,
) -> Vec<crate::types::Edge> {
    use crate::types::{Edge, EdgeKind, EdgeProvenance};
    use serde_json::json;
    let Ok(grammar) = Grammar::for_code_language(Language::Java) else {
        return vec![];
    };
    let mut parser = Parser::default();
    parser.set_language(Some(grammar));
    let project = std::path::PathBuf::from(ctx.get_project_root());
    let mut files = vec![];
    let mut listeners: HashMap<(String, String), Vec<Node>> = HashMap::new();
    for file in ctx
        .get_all_files()
        .into_iter()
        .filter(|f| f.ends_with(".java"))
    {
        let Some(source) = ctx.read_file(&file) else {
            continue;
        };
        let Some(tree) = parser.parse(&source, None) else {
            continue;
        };
        let nodes = ctx.get_nodes_in_file(&file);
        let scope = application_scope(&project, &file);
        let mut registry = ListenerRegistry::default();
        registry.register(&file, &source, &nodes);
        let events: std::collections::HashSet<_> = registry
            .files
            .values()
            .flatten()
            .map(|(event, _)| event.as_str())
            .collect();
        for event in events {
            if let Some(event_name) = event_type(&tree.root_node, event) {
                let targets = registry.targets(event);
                let entry = listeners.entry((scope.clone(), event_name)).or_default();
                for target in targets {
                    if !entry.iter().any(|n| n.id == target.id) {
                        entry.push(target.clone());
                    }
                }
            }
        }
        files.push((file, scope, tree.root_node, nodes));
    }
    for targets in listeners.values_mut() {
        targets.sort_by(|a, b| a.id.cmp(&b.id));
    }
    let mut edges = vec![];
    for (file, scope, root, nodes) in files {
        let mut seen = std::collections::HashSet::new();
        for call in root.descendants_of_type(&["method_invocation"]) {
            if call.has_error || seen.len() >= 80 {
                continue;
            }
            if !call
                .child_for_field_name("name")
                .is_some_and(|n| n.text() == "publishEvent")
            {
                continue;
            }
            if !call
                .child_for_field_name("object")
                .is_some_and(|n| static_receiver(&n))
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
            if argument.node_type() != "object_creation_expression"
                || argument
                    .named_children
                    .iter()
                    .any(|n| n.node_type() == "class_body")
            {
                continue;
            }
            let Some(ty) = argument.child_for_field_name("type") else {
                continue;
            };
            if !matches!(ty.node_type(), "type_identifier" | "scoped_type_identifier") {
                continue;
            }
            let Some(event) = event_type(&root, &ty.text()) else {
                continue;
            };
            let Some(targets) = listeners.get(&(scope.clone(), event.clone())) else {
                continue;
            };
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
                        ("synthesizedBy".into(), json!("spring-event")),
                        ("via".into(), json!(event)),
                        ("registeredAt".into(), json!(format!("{file}:{line}"))),
                    ])),
                });
            }
        }
    }
    edges
}
