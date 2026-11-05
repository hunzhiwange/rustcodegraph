//! HTTP registration evidence (upstream 15483713, Express postExtract).
//! Client dispatch is a separate pass: an unresolved mount never supplies a path.
use std::collections::{HashMap, HashSet};

use crate::{
    resolution::{import_resolver::resolve_import_path, types::ResolutionContext},
    types::{Language, Node, NodeKind},
    web_tree_sitter::{Language as Grammar, Parser, SyntaxNode},
};

fn literal(n: &SyntaxNode) -> Option<String> {
    if n.node_type() != "string" || n.has_error {
        return None;
    }
    let s = n.text();
    let value = s.get(1..s.len().checked_sub(1)?)?;
    (!value.contains('\\')).then(|| value.to_owned())
}
fn join(base: &str, path: &str) -> String {
    let parts: Vec<_> = base
        .split('/')
        .chain(path.split('/'))
        .filter(|s| !s.is_empty())
        .collect();
    format!("/{}", parts.join("/"))
}

/// Rebuild from qualified_name, never the already-prefixed display name.
/// Ambiguous mounts and their descendants are left at their original names.
pub(crate) fn normalize_routes(ctx: &mut dyn ResolutionContext) -> Vec<Node> {
    let files = ctx.get_all_files();
    let routes = ctx.get_nodes_by_kind(NodeKind::Route);
    let route_files: HashSet<_> = routes.iter().map(|r| r.file_path.clone()).collect();
    let mut registrations = HashSet::new();
    let mut mounts: HashMap<String, Vec<(String, String)>> = HashMap::new();
    for file in &files {
        let lang = if file.ends_with(".ts") || file.ends_with(".tsx") {
            Language::TypeScript
        } else if [".js", ".mjs", ".cjs"].iter().any(|s| file.ends_with(s)) {
            Language::JavaScript
        } else {
            continue;
        };
        let Some(source) = ctx.read_file(file) else {
            continue;
        };
        if !source.contains(".use") && !route_files.contains(file) {
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
        let imports = ctx.get_import_mappings(file, lang);
        let mut bindings = HashMap::new();
        for im in imports {
            bindings.insert(im.local_name, im.source);
        }
        // CommonJS bindings are read structurally; comments/strings cannot register mounts.
        for decl in tree.root_node.descendants_of_type(&["variable_declarator"]) {
            let (Some(name), Some(value)) = (
                decl.child_for_field_name("name"),
                decl.child_for_field_name("value"),
            ) else {
                continue;
            };
            if name.node_type() != "identifier" {
                continue;
            }
            if let Some(path) = required(value) {
                bindings.insert(name.text(), path);
            }
        }
        for call in tree.root_node.descendants_of_type(&["call_expression"]) {
            if call.has_error {
                continue;
            }
            let (Some(callee), Some(args)) = (
                call.child_for_field_name("function"),
                call.child_for_field_name("arguments"),
            ) else {
                continue;
            };
            if callee.node_type() != "member_expression" {
                continue;
            }
            let Some(property) = callee.child_for_field_name("property") else {
                continue;
            };
            let method = property.text();
            let args: Vec<_> = args
                .named_children
                .iter()
                .filter(|n| n.node_type() != "comment")
                .collect();
            if args.len() < 2 {
                continue;
            }
            let Some(prefix) = literal(args[0]).filter(|s| s.starts_with('/')) else {
                continue;
            };
            if matches!(
                method.as_str(),
                "get" | "post" | "put" | "patch" | "delete" | "all" | "use"
            ) && callee
                .child_for_field_name("object")
                .is_some_and(|n| matches!(n.text().as_str(), "app" | "router"))
            {
                registrations.insert((
                    file.clone(),
                    call.start_position.row as u64 + 1,
                    method.to_ascii_uppercase(),
                    prefix.clone(),
                ));
            }
            if method != "use" {
                continue;
            }
            let target = args[args.len() - 1];
            let path = required(target).or_else(|| {
                let name = if target.node_type() == "identifier" {
                    Some(target.text())
                } else if target.node_type() == "member_expression"
                    && target.child_for_field_name("property").is_some_and(|p| {
                        matches!(p.text().as_str(), "default" | "router" | "routes")
                    })
                {
                    target
                        .child_for_field_name("object")
                        .filter(|n| n.node_type() == "identifier")
                        .map(|n| n.text())
                } else {
                    None
                }?;
                bindings.get(&name).cloned()
            });
            let Some(target) = path.and_then(|p| resolve_import_path(&p, file, lang, ctx)) else {
                continue;
            };
            if target != *file {
                mounts
                    .entry(file.clone())
                    .or_default()
                    .push((prefix, target));
            }
        }
    }
    let incoming: HashSet<_> = mounts.values().flatten().map(|(_, t)| t.clone()).collect();
    let mut paths: HashMap<String, HashSet<String>> = HashMap::new();
    // Walk from roots rather than treating an ambiguous intermediate as a new root.
    for root in mounts.keys().filter(|f| !incoming.contains(*f)) {
        walk(root, "", &mounts, &mut HashSet::new(), &mut paths);
    }
    let mut updates = Vec::new();
    for mut route in routes {
        let Some((_, original)) = route.qualified_name.split_once("::") else {
            continue;
        };
        let Some((method, original)) = original.split_once(':') else {
            continue;
        };
        if !registrations.contains(&(
            route.file_path.clone(),
            route.start_line,
            method.to_owned(),
            original.to_owned(),
        )) {
            continue;
        }
        let path = match paths.get(&route.file_path) {
            Some(p) if p.len() == 1 => join(p.iter().next().unwrap(), original),
            _ => original.to_owned(),
        };
        let name = format!("{method} {path}");
        if route.name != name {
            route.name = name;
            updates.push(route);
        }
    }
    updates
}
fn required(n: &SyntaxNode) -> Option<String> {
    if n.node_type() != "call_expression"
        || n.has_error
        || n.child_for_field_name("function")?.text() != "require"
    {
        return None;
    }
    let args = n.child_for_field_name("arguments")?;
    if args.named_children.len() != 1 {
        return None;
    }
    literal(&args.named_children[0])
}
fn walk(
    file: &str,
    base: &str,
    mounts: &HashMap<String, Vec<(String, String)>>,
    visiting: &mut HashSet<String>,
    paths: &mut HashMap<String, HashSet<String>>,
) {
    if !visiting.insert(file.to_owned()) {
        return;
    }
    if let Some(children) = mounts.get(file) {
        for (prefix, target) in children {
            let full = join(base, prefix);
            // Two paths suffice to prove ambiguity; cap expansion in diamond graphs.
            let known = paths.entry(target.clone()).or_default();
            if known.len() < 2 && known.insert(full.clone()) {
                walk(target, &full, mounts, visiting, paths);
            }
        }
    }
    visiting.remove(file);
}
