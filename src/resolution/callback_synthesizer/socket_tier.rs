//! Socket.IO registration evidence (upstream tier-synthesizer.ts @15483713).
//! A socket-shaped name alone is never transport or direction evidence.
use super::common::{EVENT_FANOUT_CAP, edge, enclosing_fn};
use crate::{
    resolution::types::ResolutionContext,
    types::{Edge, EdgeKind, Language, Node, NodeKind},
    web_tree_sitter::{Language as Grammar, Parser, SyntaxNode},
};
use serde_json::json;
use std::collections::{HashMap, HashSet};

fn field(n: &SyntaxNode, key: &str) -> Option<String> {
    n.child_for_field_name(key).map(|n| n.text())
}
fn args(n: &SyntaxNode) -> Vec<&SyntaxNode> {
    n.child_for_field_name("arguments")
        .map(|n| {
            n.named_children
                .iter()
                .filter(|n| n.node_type() != "comment")
                .collect()
        })
        .unwrap_or_default()
}
fn literal(n: &SyntaxNode) -> Option<String> {
    if n.node_type() != "string" || n.has_error {
        return None;
    }
    let text = n.text();
    let value = text.get(1..text.len().checked_sub(1)?)?;
    (!value.is_empty() && !value.contains('\\')).then(|| value.to_owned())
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
        "required_parameter",
        "optional_parameter",
        "catch_clause",
    ]) {
        let target = n
            .child_for_field_name("name")
            .or_else(|| n.child_for_field_name("pattern"))
            .or_else(|| n.child_for_field_name("left"))
            .or_else(|| n.child_for_field_name("argument"))
            .or_else(|| n.child_for_field_name("parameter"));
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
    for n in root.descendants_of_type(&["arrow_function", "formal_parameters"]) {
        if n.child_for_field_name("parameter")
            .is_some_and(|n| n.text() == name)
        {
            count += 1;
        }
        if n.node_type() == "formal_parameters" {
            count += n
                .named_children
                .iter()
                .filter(|n| n.node_type() == "identifier" && n.text() == name)
                .count();
        }
    }
    count
}
fn parse(file: &str, source: &str) -> Option<SyntaxNode> {
    let lang = if file.ends_with(".ts") || file.ends_with(".tsx") {
        Language::TypeScript
    } else if [".js", ".jsx", ".mjs", ".cjs"]
        .iter()
        .any(|s| file.ends_with(s))
    {
        Language::JavaScript
    } else {
        return None;
    };
    let mut parser = Parser::default();
    parser.set_language(Some(Grammar::for_code_language(lang).ok()?));
    Some(parser.parse(source, None)?.root_node)
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Side {
    Client,
    Server,
}
struct File {
    root: SyntaxNode,
    nodes: Vec<Node>,
    receivers: HashMap<String, (Side, usize, usize)>,
    scope: String,
    imports: Vec<(String, String, String)>,
    exports: HashSet<String>,
}
struct Site {
    file: String,
    node: Node,
    line: u64,
    event: String,
    side: Side,
    callee: String,
}
fn generic(event: &str) -> bool {
    matches!(
        event,
        "error"
            | "connect"
            | "connect_error"
            | "connect_failed"
            | "connection"
            | "disconnect"
            | "disconnecting"
            | "reconnect"
            | "reconnect_attempt"
            | "reconnecting"
            | "reconnect_error"
            | "reconnect_failed"
            | "close"
            | "open"
            | "end"
            | "data"
            | "ready"
            | "drain"
            | "finish"
            | "pipe"
            | "unpipe"
            | "listening"
            | "timeout"
            | "ping"
            | "pong"
            | "upgrade"
            | "newListener"
            | "removeListener"
    )
}
fn handler(data: &File, callback: &SyntaxNode, line: u64) -> Option<Node> {
    if callback.node_type() == "identifier" {
        let name = callback.text();
        if binding_count(&data.root, &name) != 1 {
            return None;
        }
        let declarations: Vec<_> = data
            .root
            .descendants_of_type(&["function_declaration", "variable_declarator"])
            .into_iter()
            .filter(|n| {
                field(n, "name").as_deref() == Some(&name)
                    && !n.has_error
                    && (if n.node_type() == "function_declaration" {
                        n.parent().is_some_and(|p| {
                            matches!(p.node_type(), "program" | "export_statement")
                        })
                    } else {
                        crate::extraction::languages::typescript::rtk::is_module_declaration(n)
                    })
                    && (n.node_type() == "function_declaration"
                        || n.child_for_field_name("value").is_some_and(|n| {
                            matches!(n.node_type(), "arrow_function" | "function_expression")
                        }))
            })
            .collect();
        if declarations.len() != 1 {
            return None;
        }
        let start = declarations[0].start_position.row as u64 + 1;
        let nodes: Vec<_> = data
            .nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Function && n.name == name && n.start_line == start)
            .collect();
        return (nodes.len() == 1).then(|| nodes[0].clone());
    }
    if matches!(
        callback.node_type(),
        "arrow_function" | "function_expression"
    ) {
        // Inline listeners land in their enclosing named callable, as upstream.
        return enclosing_fn(&data.nodes, line);
    }
    None
}
// Resolve only explicit named exports through one unambiguous direct import.
// Never fall back to a global function name (monorepos commonly repeat them).
fn registered_handler(
    files: &HashMap<String, File>,
    file: &str,
    callback: &SyntaxNode,
    line: u64,
) -> Option<Node> {
    let data = files.get(file)?;
    if let Some(node) = handler(data, callback, line) {
        return Some(node);
    }
    if callback.node_type() != "identifier" || binding_count(&data.root, &callback.text()) != 0 {
        return None;
    }
    let imports: Vec<_> = data
        .imports
        .iter()
        .filter(|(local, _, _)| local == &callback.text())
        .collect();
    if imports.len() != 1 {
        return None;
    }
    let (_, name, path) = imports[0];
    let target = files.get(path)?;
    if target.scope != data.scope || !target.exports.contains(name) {
        return None;
    }
    let declaration = target
        .root
        .descendants_of_type(&["function_declaration", "variable_declarator"])
        .into_iter()
        .find(|n| field(n, "name").as_deref() == Some(name))?;
    handler(target, declaration.child_for_field_name("name")?, line)
}
pub(super) fn dispatch_edges(ctx: &mut dyn ResolutionContext) -> Vec<Edge> {
    let project = std::path::PathBuf::from(ctx.get_project_root());
    let mut files = HashMap::new();
    let paths = ctx.get_all_files();
    let mut selected = HashSet::new();
    for file in &paths {
        let Some(source) = ctx.read_file(file) else {
            continue;
        };
        if !source.contains("socket.io") {
            continue;
        }
        let lang = if file.ends_with(".ts") || file.ends_with(".tsx") {
            Language::TypeScript
        } else {
            Language::JavaScript
        };
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
        let Some(source) = ctx.read_file(&file) else {
            continue;
        };
        let Some(root) = parse(&file, &source) else {
            continue;
        };
        let lang = if file.ends_with(".ts") || file.ends_with(".tsx") {
            Language::TypeScript
        } else {
            Language::JavaScript
        };
        let imports = ctx.get_import_mappings(&file, lang);
        let factories: HashMap<_, _> = imports
            .iter()
            .filter_map(|i| {
                if binding_count(&root, &i.local_name) != 0
                    || i.is_namespace
                    || imports
                        .iter()
                        .filter(|other| other.local_name == i.local_name)
                        .count()
                        != 1
                {
                    return None;
                }
                if i.source == "socket.io-client"
                    && (i.is_default || matches!(i.exported_name.as_str(), "io" | "connect"))
                {
                    Some((i.local_name.clone(), Side::Client))
                } else if i.source == "socket.io" && i.exported_name == "Server" {
                    Some((i.local_name.clone(), Side::Server))
                } else {
                    None
                }
            })
            .collect();
        let mut receivers = HashMap::new();
        for d in root.descendants_of_type(&["variable_declarator"]) {
            if !crate::extraction::languages::typescript::rtk::is_module_declaration(d) {
                continue;
            }
            let (Some(name), Some(value)) = (
                d.child_for_field_name("name"),
                d.child_for_field_name("value"),
            ) else {
                continue;
            };
            if name.node_type() != "identifier"
                || binding_count(&root, &name.text()) != 1
                || value.has_error
            {
                continue;
            }
            let factory = match value.node_type() {
                "call_expression" => field(value, "function"),
                "new_expression" => field(value, "constructor"),
                _ => None,
            };
            if let Some(side) = factory.and_then(|n| factories.get(&n).copied()) {
                if (side == Side::Client && value.node_type() == "call_expression")
                    || (side == Side::Server && value.node_type() == "new_expression")
                {
                    receivers.insert(name.text(), (side, 0, source.len()));
                }
            }
        }
        // A connected server-side socket inherits its direction only inside the
        // registered connection callback; the parameter name has no global meaning.
        let mut connected = vec![];
        for call in root.descendants_of_type(&["call_expression"]) {
            if call.has_error {
                continue;
            }
            let Some(callee) = call
                .child_for_field_name("function")
                .filter(|n| n.node_type() == "member_expression")
            else {
                continue;
            };
            let Some(object) = field(callee, "object") else {
                continue;
            };
            if receivers.get(&object).map(|r| r.0) != Some(Side::Server)
                || field(callee, "property").as_deref() != Some("on")
            {
                continue;
            }
            let a = args(call);
            if a.first().and_then(|n| literal(n)).as_deref() != Some("connection") {
                continue;
            }
            let Some(callback) = a
                .get(1)
                .filter(|n| matches!(n.node_type(), "arrow_function" | "function_expression"))
            else {
                continue;
            };
            let parameter = callback.child_for_field_name("parameter").or_else(|| {
                callback
                    .child_for_field_name("parameters")
                    .and_then(|n| n.named_children.first())
                    .and_then(|n| {
                        if n.node_type() == "required_parameter" {
                            n.child_for_field_name("pattern")
                        } else {
                            Some(n)
                        }
                    })
            });
            if let Some(p) = parameter
                .filter(|n| n.node_type() == "identifier" && binding_count(&root, &n.text()) == 1)
            {
                connected.push((
                    p.text(),
                    (Side::Server, callback.start_index, callback.end_index),
                ));
            }
        }
        receivers.extend(connected);
        let local_imports = imports
            .into_iter()
            .filter(|i| !i.is_namespace && !i.is_default)
            .filter_map(|i| {
                let path = i.resolved_path.or_else(|| {
                    crate::resolution::import_resolver::resolve_import_path(
                        &i.source, &file, lang, ctx,
                    )
                })?;
                Some((i.local_name, i.exported_name, path))
            })
            .collect();
        let mut exports = HashSet::new();
        for e in root.named_children.iter().filter(|n| {
            n.node_type() == "export_statement" && !n.text().starts_with("export default")
        }) {
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
        let mut dir = std::path::Path::new(&file).parent();
        let mut scope = String::new();
        while let Some(p) = dir {
            if project.join(p).join("package.json").is_file() {
                scope = p.to_string_lossy().into_owned();
                break;
            }
            dir = p.parent();
        }
        files.insert(
            file.clone(),
            File {
                root,
                receivers,
                nodes: ctx.get_nodes_in_file(&file),
                scope,
                imports: local_imports,
                exports,
            },
        );
    }
    let mut sends = vec![];
    let mut listeners = vec![];
    for (file, data) in &files {
        for call in data.root.descendants_of_type(&["call_expression"]) {
            if call.has_error {
                continue;
            }
            let Some(callee) = call
                .child_for_field_name("function")
                .filter(|n| n.node_type() == "member_expression")
            else {
                continue;
            };
            let Some(receiver) = callee
                .child_for_field_name("object")
                .filter(|n| n.node_type() == "identifier")
            else {
                continue;
            };
            let Some((side, start, end)) = data.receivers.get(&receiver.text()) else {
                continue;
            };
            if call.start_index < *start || call.end_index > *end {
                continue;
            }
            let arguments = args(call);
            let Some(event) = arguments
                .first()
                .and_then(|n| literal(n))
                .filter(|n| !generic(n))
            else {
                continue;
            };
            let method = field(callee, "property").unwrap_or_default();
            let line = call.start_position.row as u64 + 1;
            let node = if method == "emit" {
                enclosing_fn(&data.nodes, line)
            } else if matches!(method.as_str(), "on" | "once") {
                arguments
                    .get(1)
                    .and_then(|n| registered_handler(&files, file, n, line))
            } else {
                None
            };
            if let Some(node) = node {
                let site = Site {
                    file: file.clone(),
                    node,
                    line,
                    event,
                    side: *side,
                    callee: callee.text(),
                };
                if method == "emit" {
                    sends.push(site);
                } else {
                    listeners.push(site);
                }
            }
        }
    }
    let mut out = vec![];
    let mut seen = HashSet::new();
    for send in &sends {
        let matched: Vec<_> = listeners
            .iter()
            .filter(|h| {
                h.side != send.side
                    && h.event == send.event
                    && files[&h.file].scope == files[&send.file].scope
            })
            .collect();
        if matched.len() > EVENT_FANOUT_CAP
            || sends
                .iter()
                .filter(|d| {
                    d.event == send.event
                        && d.side == send.side
                        && files[&d.file].scope == files[&send.file].scope
                })
                .count()
                > EVENT_FANOUT_CAP
        {
            continue;
        }
        for h in matched {
            if send.node.id == h.node.id || !seen.insert((send.node.id.clone(), h.node.id.clone()))
            {
                continue;
            }
            out.push(edge(
                &send.node.id,
                &h.node.id,
                EdgeKind::Calls,
                Some(send.line),
                "event-bus",
                [
                    ("channel", json!("socket")),
                    ("event", json!(send.event)),
                    ("callee", json!(send.callee)),
                    (
                        "tier",
                        json!(if send.side == Side::Client {
                            "client→server"
                        } else {
                            "server→client"
                        }),
                    ),
                    ("registeredAt", json!(format!("{}:{}", h.file, h.line))),
                ],
            ));
        }
    }
    out
}
