//! Named queue registration and dispatch, based on upstream 15483713.
//! Exact AST evidence only: no queue-shaped name or global handler fallback.
use super::common::{edge, enclosing_fn};
use crate::{
    resolution::types::ResolutionContext,
    types::{Edge, EdgeKind, Language, Node, NodeKind},
    web_tree_sitter::{Language as Grammar, Parser, SyntaxNode},
};
use serde_json::json;
use std::collections::{HashMap, HashSet};

fn literal(n: &SyntaxNode) -> Option<String> {
    if n.node_type() != "string" || n.has_error {
        return None;
    }
    let s = n.text();
    let value = s.get(1..s.len().checked_sub(1)?)?;
    (!value.is_empty() && !value.contains('\\')).then(|| value.to_owned())
}
fn args(n: &SyntaxNode) -> Vec<&SyntaxNode> {
    n.child_for_field_name("arguments")
        .map(|a| {
            a.named_children
                .iter()
                .filter(|n| n.node_type() != "comment")
                .collect()
        })
        .unwrap_or_default()
}
fn field(n: &SyntaxNode, name: &str) -> Option<String> {
    n.child_for_field_name(name).map(|n| n.text())
}
fn binding_count(root: &SyntaxNode, name: &str) -> usize {
    let mut count = 0;
    for n in root.descendants_of_type(&[
        "variable_declarator",
        "function_declaration",
        "class_declaration",
        "assignment_expression",
        "augmented_assignment_expression",
        "update_expression",
    ]) {
        let target = n
            .child_for_field_name("name")
            .or_else(|| n.child_for_field_name("pattern"))
            .or_else(|| n.child_for_field_name("left"))
            .or_else(|| n.child_for_field_name("argument"));
        if target.is_some_and(|n| {
            n.text() == name
                || (matches!(n.node_type(), "object_pattern" | "array_pattern")
                    && n.descendants_of_type(&[
                        "identifier",
                        "shorthand_property_identifier_pattern",
                    ])
                    .iter()
                    .any(|n| n.text() == name))
        }) {
            count += 1;
        }
    }
    for arrow in root.descendants_of_type(&["arrow_function", "catch_clause"]) {
        if arrow
            .child_for_field_name("parameter")
            .is_some_and(|n| n.text() == name)
        {
            count += 1;
        }
    }
    for params in root.descendants_of_type(&["formal_parameters"]) {
        if params
            .descendants_of_type(&["identifier", "shorthand_property_identifier_pattern"])
            .iter()
            .any(|n| n.text() == name)
        {
            count += 1;
        }
    }
    count
}
struct File {
    root: SyntaxNode,
    nodes: Vec<Node>,
    queues: HashMap<String, String>,
    workers: HashSet<String>,
    imports: Vec<(String, String, String)>,
    exports: HashSet<String>,
    scope: String,
}
struct Consumer {
    file: String,
    queue: String,
    job: Option<String>,
    node: Node,
    line: u64,
}
fn exported(data: &File, name: &str) -> bool {
    data.exports.contains(name)
}
fn callable(data: &File, name: &str) -> Option<Node> {
    if binding_count(&data.root, name) != 1 {
        return None;
    }
    let declarations: Vec<_> = data
        .root
        .descendants_of_type(&["function_declaration", "variable_declarator"])
        .into_iter()
        .filter(|d| {
            if field(d, "name").as_deref() != Some(name) {
                return false;
            }
            if d.node_type() == "function_declaration" {
                return d
                    .parent()
                    .is_some_and(|p| matches!(p.node_type(), "program" | "export_statement"));
            }
            crate::extraction::languages::typescript::rtk::is_module_declaration(d)
                && d.child_for_field_name("value").is_some_and(|v| {
                    matches!(v.node_type(), "arrow_function" | "function_expression")
                })
        })
        .collect();
    if declarations.len() != 1 {
        return None;
    }
    let line = declarations[0].start_position.row as u64 + 1;
    let matches: Vec<_> = data
        .nodes
        .iter()
        .filter(|v| v.kind == NodeKind::Function && v.name == name && v.start_line == line)
        .collect();
    (matches.len() == 1).then(|| matches[0].clone())
}
fn handler(files: &HashMap<String, File>, file: &str, n: &SyntaxNode) -> Option<Node> {
    if n.node_type() != "identifier" {
        return None;
    }
    let data = files.get(file)?;
    let name = n.text();
    if let Some(node) = callable(data, &name) {
        return Some(node);
    }
    if binding_count(&data.root, &name) != 0 {
        return None;
    }
    let imports: Vec<_> = data
        .imports
        .iter()
        .filter(|(local, _, _)| local == &name)
        .collect();
    if imports.len() != 1 {
        return None;
    }
    let (_, name, path) = imports[0];
    let target = files.get(path)?;
    if target.scope != data.scope || !exported(target, name) {
        return None;
    }
    callable(target, name)
}
fn queue_for(files: &HashMap<String, File>, file: &str, name: &str) -> Option<String> {
    let data = files.get(file)?;
    if let Some(q) = data.queues.get(name) {
        return Some(q.clone());
    }
    if binding_count(&data.root, name) != 0 {
        return None;
    }
    let imports: Vec<_> = data
        .imports
        .iter()
        .filter(|(local, _, _)| local == name)
        .collect();
    if imports.len() != 1 {
        return None;
    }
    let (_, name, path) = imports[0];
    let target = files.get(path)?;
    if target.scope != data.scope || !exported(target, name) {
        return None;
    }
    target.queues.get(name).cloned()
}
fn collect(ctx: &mut dyn ResolutionContext) -> HashMap<String, File> {
    let mut files = HashMap::new();
    let project = std::path::PathBuf::from(ctx.get_project_root());
    // Parse only queue-shaped sources and their direct imports. The direct
    // dependency pass supplies queue exports and named handlers without reparsing
    // every unrelated JS/TS source in a large repository.
    let paths = ctx.get_all_files();
    let language = |file: &str| {
        if file.ends_with(".ts") || file.ends_with(".tsx") {
            Some(Language::TypeScript)
        } else if [".js", ".jsx", ".mjs", ".cjs"]
            .iter()
            .any(|s| file.ends_with(s))
        {
            Some(Language::JavaScript)
        } else {
            None
        }
    };
    let mut selected = HashSet::new();
    for file in &paths {
        let Some(lang) = language(file) else {
            continue;
        };
        let Some(source) = ctx.read_file(file) else {
            continue;
        };
        if !["bull", "add", "process"]
            .iter()
            .any(|s| source.contains(s))
        {
            continue;
        }
        selected.insert(file.clone());
        for i in ctx.get_import_mappings(file, lang) {
            if let Some(path) = i.resolved_path.or_else(|| {
                crate::resolution::import_resolver::resolve_import_path(&i.source, file, lang, ctx)
            }) {
                selected.insert(path);
            }
        }
    }
    for file in paths.into_iter().filter(|f| selected.contains(f)) {
        let lang = if file.ends_with(".ts") || file.ends_with(".tsx") {
            Language::TypeScript
        } else if [".js", ".jsx", ".mjs", ".cjs"]
            .iter()
            .any(|s| file.ends_with(s))
        {
            Language::JavaScript
        } else {
            continue;
        };
        let Some(source) = ctx.read_file(&file) else {
            continue;
        };
        let Ok(grammar) = Grammar::for_code_language(lang) else {
            continue;
        };
        let mut parser = Parser::default();
        parser.set_language(Some(grammar));
        let Some(tree) = parser.parse(&source, None) else {
            continue;
        };
        let imports = ctx.get_import_mappings(&file, lang);
        let root = tree.root_node;
        let constructors: HashSet<_> = imports
            .iter()
            .filter(|i| {
                binding_count(&root, &i.local_name) == 0
                    && ((i.source == "bullmq" && i.exported_name == "Queue")
                        || (i.source == "bull" && (i.is_default || i.exported_name == "Queue")))
            })
            .map(|i| i.local_name.clone())
            .collect();
        let workers = imports
            .iter()
            .filter(|i| {
                i.source == "bullmq"
                    && i.exported_name == "Worker"
                    && binding_count(&root, &i.local_name) == 0
            })
            .map(|i| i.local_name.clone())
            .collect();
        let mut queues = HashMap::new();
        for decl in root.descendants_of_type(&["variable_declarator"]) {
            if !crate::extraction::languages::typescript::rtk::is_module_declaration(decl) {
                continue;
            }
            let (Some(name), Some(value)) = (
                decl.child_for_field_name("name"),
                decl.child_for_field_name("value"),
            ) else {
                continue;
            };
            if name.node_type() != "identifier"
                || binding_count(&root, &name.text()) != 1
                || value.node_type() != "new_expression"
                || value.has_error
            {
                continue;
            }
            if !field(value, "constructor").is_some_and(|n| constructors.contains(&n)) {
                continue;
            }
            if let Some(queue) = args(value).first().and_then(|n| literal(n)) {
                queues.insert(name.text(), queue);
            }
        }
        let local_imports = imports
            .into_iter()
            .filter_map(|i| {
                if i.is_namespace {
                    return None;
                }
                let path = i.resolved_path.or_else(|| {
                    crate::resolution::import_resolver::resolve_import_path(
                        &i.source, &file, lang, ctx,
                    )
                })?;
                Some((i.local_name, i.exported_name, path))
            })
            .collect();
        let mut dir = std::path::Path::new(&file).parent();
        let mut scope = String::new();
        while let Some(p) = dir {
            if project.join(p).join("package.json").is_file() {
                scope = p.to_string_lossy().into_owned();
                break;
            }
            dir = p.parent();
        }
        let mut exports = HashSet::new();
        for e in root
            .named_children
            .iter()
            .filter(|n| n.node_type() == "export_statement")
        {
            if e.text().starts_with("export default") {
                continue;
            }
            for child in &e.named_children {
                if child.node_type() == "function_declaration" {
                    if let Some(name) = field(child, "name") {
                        exports.insert(name);
                    }
                } else if matches!(
                    child.node_type(),
                    "lexical_declaration" | "variable_declaration"
                ) {
                    for d in child.descendants_of_type(&["variable_declarator"]) {
                        if let Some(name) = d
                            .child_for_field_name("name")
                            .filter(|n| n.node_type() == "identifier")
                        {
                            exports.insert(name.text());
                        }
                    }
                }
            }
        }
        files.insert(
            file.clone(),
            File {
                root,
                nodes: ctx.get_nodes_in_file(&file),
                queues,
                workers,
                imports: local_imports,
                exports,
                scope,
            },
        );
    }
    files
}
fn consumers(files: &HashMap<String, File>) -> Vec<Consumer> {
    let mut out = vec![];
    for (file, data) in files {
        for call in data
            .root
            .descendants_of_type(&["call_expression", "new_expression"])
        {
            if call.has_error {
                continue;
            }
            let arguments = args(call);
            let found = if call.node_type() == "new_expression"
                && field(call, "constructor").is_some_and(|n| data.workers.contains(&n))
            {
                if arguments.len() < 2 {
                    continue;
                }
                literal(arguments[0]).map(|q| (q, None, arguments[1]))
            } else {
                let Some(callee) = call.child_for_field_name("function") else {
                    continue;
                };
                if callee.node_type() != "member_expression"
                    || field(callee, "property").as_deref() != Some("process")
                {
                    continue;
                }
                let Some(object) = callee
                    .child_for_field_name("object")
                    .filter(|n| n.node_type() == "identifier")
                else {
                    continue;
                };
                let Some(queue) = queue_for(files, file, &object.text()) else {
                    continue;
                };
                match arguments.as_slice() {
                    [callback] => Some((queue.clone(), None, *callback)),
                    [job, callback] => literal(job).map(|j| (queue.clone(), Some(j), *callback)),
                    _ => None,
                }
            };
            if let Some((queue, job, callback)) = found {
                if let Some(node) = handler(files, file, callback) {
                    out.push(Consumer {
                        file: file.clone(),
                        queue,
                        job,
                        node,
                        line: call.start_position.row as u64 + 1,
                    });
                }
            }
        }
    }
    out
}
pub(super) fn dispatch_edges(ctx: &mut dyn ResolutionContext) -> Vec<Edge> {
    let files = collect(ctx);
    let consumers = consumers(&files);
    let mut edges = vec![];
    let mut seen = HashSet::new();
    for (file, data) in &files {
        for call in data.root.descendants_of_type(&["call_expression"]) {
            if call.has_error {
                continue;
            }
            let Some(callee) = call.child_for_field_name("function") else {
                continue;
            };
            if callee.node_type() != "member_expression"
                || field(callee, "property").as_deref() != Some("add")
            {
                continue;
            }
            let Some(object) = callee
                .child_for_field_name("object")
                .filter(|n| n.node_type() == "identifier")
            else {
                continue;
            };
            let Some(queue) = queue_for(&files, file, &object.text()) else {
                continue;
            };
            let Some(job) = args(call).first().and_then(|n| literal(n)) else {
                continue;
            };
            let line = call.start_position.row as u64 + 1;
            let Some(owner) = enclosing_fn(&data.nodes, line) else {
                continue;
            };
            let candidates: Vec<_> = consumers
                .iter()
                .filter(|c| {
                    files[&c.file].scope == data.scope
                        && c.queue == queue
                        && (c.job.is_none() || c.job.as_ref() == Some(&job))
                })
                .collect();
            let exact: Vec<_> = candidates
                .iter()
                .copied()
                .filter(|c| c.job.as_ref() == Some(&job))
                .collect();
            let candidates = if exact.is_empty() { candidates } else { exact };
            if candidates.len() != 1 {
                continue;
            }
            let c = candidates[0];
            if owner.id == c.node.id || !seen.insert((owner.id.clone(), c.node.id.clone())) {
                continue;
            }
            edges.push(edge(
                &owner.id,
                &c.node.id,
                EdgeKind::Calls,
                Some(line),
                "queue-job",
                [
                    ("channel", json!("queue")),
                    ("queue", json!(queue)),
                    ("event", json!(job)),
                    ("registeredAt", json!(format!("{}:{}", c.file, c.line))),
                ],
            ));
        }
    }
    edges
}
