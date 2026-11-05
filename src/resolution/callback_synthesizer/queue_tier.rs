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
        if target.is_some_and(|n| n.text() == name) {
            count += 1;
        }
    }
    for params in root.descendants_of_type(&["formal_parameters"]) {
        if params
            .descendants_of_type(&["identifier"])
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
}
struct Consumer {
    queue: String,
    job: Option<String>,
    node: Node,
    line: u64,
}
fn handler(data: &File, n: &SyntaxNode) -> Option<Node> {
    if n.node_type() != "identifier" {
        return None;
    }
    let matches: Vec<_> = data
        .nodes
        .iter()
        .filter(|v| v.kind == NodeKind::Function && v.name == n.text())
        .collect();
    (matches.len() == 1).then(|| matches[0].clone())
}
fn collect(ctx: &mut dyn ResolutionContext) -> HashMap<String, File> {
    let mut files = HashMap::new();
    for file in ctx.get_all_files() {
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
        if !source.contains("bull") {
            continue;
        }
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
        files.insert(
            file.clone(),
            File {
                root,
                nodes: ctx.get_nodes_in_file(&file),
                queues,
                workers,
            },
        );
    }
    files
}
fn consumers(files: &HashMap<String, File>) -> Vec<Consumer> {
    let mut out = vec![];
    for data in files.values() {
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
                let Some(queue) = data.queues.get(&object.text()) else {
                    continue;
                };
                match arguments.as_slice() {
                    [callback] => Some((queue.clone(), None, *callback)),
                    [job, callback] => literal(job).map(|j| (queue.clone(), Some(j), *callback)),
                    _ => None,
                }
            };
            if let Some((queue, job, callback)) = found {
                if let Some(node) = handler(data, callback) {
                    out.push(Consumer {
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
            let Some(queue) = data.queues.get(&object.text()) else {
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
                    c.node.file_path == *file
                        && c.queue == *queue
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
                    (
                        "registeredAt",
                        json!(format!("{}:{}", c.node.file_path, c.line)),
                    ),
                ],
            ));
        }
    }
    edges
}
