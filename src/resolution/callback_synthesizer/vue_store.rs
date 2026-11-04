//! Vue store dispatch based on AST registrations and exact local/import bindings.
//! Source: upstream 15483713 piniaStoreEdges/vuexDispatchEdges. No global-name fallback.
use super::common::{edge, enclosing_fn};
use crate::{
    resolution::types::ResolutionContext,
    types::{Edge, EdgeKind, Language, Node},
    web_tree_sitter::{Language as Grammar, Parser, SyntaxNode},
};
use serde_json::json;
use std::collections::{HashMap, HashSet};

fn field(n: &SyntaxNode, f: &str) -> Option<String> {
    n.child_for_field_name(f).map(|n| n.text())
}
fn key(n: &SyntaxNode) -> Option<String> {
    n.child_for_field_name("key")
        .or_else(|| n.child_for_field_name("name"))
        .filter(|k| {
            matches!(
                k.node_type(),
                "identifier" | "property_identifier" | "string"
            )
        })
        .map(|k| k.text().trim_matches(['\'', '"']).to_owned())
}
struct File {
    root: SyntaxNode,
    nodes: Vec<Node>,
    imports: Vec<(String, String, String)>,
}
#[derive(Clone)]
struct Store {
    file: String,
    binding: String,
    pinia: bool,
    members: Vec<(String, String, Node)>,
}
fn declarations(root: &SyntaxNode) -> Vec<&SyntaxNode> {
    root.descendants_of_type(&["variable_declarator"])
        .into_iter()
        .filter(|n| crate::extraction::languages::typescript::rtk::is_module_declaration(n))
        .collect()
}
fn object(
    files: &HashMap<String, File>,
    file: &str,
    value: &SyntaxNode,
    depth: usize,
) -> Option<(String, SyntaxNode)> {
    if depth > 8 {
        return None;
    }
    if value.node_type() == "object" {
        return Some((file.to_owned(), value.clone()));
    }
    if !matches!(
        value.node_type(),
        "identifier" | "shorthand_property_identifier"
    ) {
        return None;
    }
    let data = files.get(file)?;
    let name = value.text();
    if !stable(&data.root, &name) {
        return None;
    }
    let decls: Vec<_> = declarations(&data.root)
        .into_iter()
        .filter(|d| field(d, "name").as_deref() == Some(&name))
        .collect();
    if decls.len() == 1 {
        return object(
            files,
            file,
            decls[0].child_for_field_name("value")?,
            depth + 1,
        );
    }
    let imports: Vec<_> = data
        .imports
        .iter()
        .filter(|(local, _, _)| local == &name)
        .collect();
    if imports.len() != 1 || imports[0].1 != "default" {
        return None;
    }
    let target = &imports[0].2;
    let root = &files.get(target)?.root;
    let export = root
        .named_children
        .iter()
        .find(|n| n.node_type() == "export_statement" && n.text().starts_with("export default"))?;
    object(files, target, export.named_children.first()?, depth + 1)
}
fn members(
    files: &HashMap<String, File>,
    file: &str,
    config: &SyntaxNode,
    namespace: &str,
    depth: usize,
) -> Vec<(String, String, Node)> {
    if depth > 8 {
        return vec![];
    }
    let mut out = Vec::new();
    let data = &files[file];
    for (name, collection, function) in
        crate::extraction::languages::typescript::vue_store::registrations(&data.root)
    {
        // Collections may be declared before the config and registered by shorthand.
        let belongs = config
            .named_children
            .iter()
            .filter(|n| {
                key(n)
                    .or_else(|| {
                        (n.node_type() == "shorthand_property_identifier").then(|| n.text())
                    })
                    .as_deref()
                    == Some(&collection)
            })
            .filter_map(|n| object(files, file, n.child_for_field_name("value").unwrap_or(n), 0))
            .any(|(path, object)| {
                path == file
                    && function.start_index >= object.start_index
                    && function.end_index <= object.end_index
            });
        if !belongs {
            continue;
        }
        let nodes: Vec<_> = data
            .nodes
            .iter()
            .filter(|n| {
                n.name == name
                    && n.start_line == function.start_position.row as u64 + 1
                    && n.start_column == function.start_position.column as u64
                    && n.signature.as_deref() == Some(&format!("= Vue store {collection}"))
            })
            .collect();
        if nodes.len() == 1 {
            out.push((format!("{namespace}{name}"), collection, nodes[0].clone()));
        }
    }
    if let Some(modules) = config
        .named_children
        .iter()
        .find(|n| key(n).as_deref() == Some("modules"))
        .and_then(|n| n.child_for_field_name("value"))
    {
        if let Some((mf, modules)) = object(files, file, modules, 0) {
            for member in &modules.named_children {
                let Some(name) = key(member).or_else(|| {
                    (member.node_type() == "shorthand_property_identifier").then(|| member.text())
                }) else {
                    continue;
                };
                if let Some((f, c)) = object(
                    files,
                    &mf,
                    member.child_for_field_name("value").unwrap_or(member),
                    0,
                ) {
                    let namespaced = c.named_children.iter().any(|n| {
                        key(n).as_deref() == Some("namespaced")
                            && field(n, "value").as_deref() == Some("true")
                    });
                    let ns = if namespaced {
                        format!("{namespace}{name}/")
                    } else {
                        namespace.to_owned()
                    };
                    out.extend(members(files, &f, &c, &ns, depth + 1));
                }
            }
        }
    }
    out
}
fn binding_visible(files: &HashMap<String, File>, file: &str, name: &str, store: &Store) -> bool {
    (store.file == file && store.binding == name)
        || files[file].imports.iter().any(|(local, exported, path)| {
            local == name && exported == &store.binding && path == &store.file
        })
}
// Reject uncertain scopes and reassignment rather than select a global store by name.
fn stable(scope: &SyntaxNode, name: &str) -> bool {
    !scope
        .descendants_of_type(&[
            "assignment_expression",
            "augmented_assignment_expression",
            "update_expression",
        ])
        .iter()
        .any(|n| {
            field(n, "left")
                .or_else(|| field(n, "argument"))
                .is_some_and(|s| s == name)
        })
}
fn shadowed(scope: &SyntaxNode, name: &str) -> bool {
    scope.child_for_field_name("parameters").is_some_and(|p| {
        p.descendants_of_type(&["identifier", "shorthand_property_identifier_pattern"])
            .iter()
            .any(|n| n.text() == name)
    }) || scope
        .descendants_of_type(&["variable_declarator", "function_declaration"])
        .iter()
        .any(|n| field(n, "name").as_deref() == Some(name))
}

pub(super) fn vue_store_edges(ctx: &mut dyn ResolutionContext) -> Vec<Edge> {
    // Retain ASTs only for potential registrations and their direct consumers,
    // rather than retaining every JS/TS AST in the workspace.
    let paths = ctx.get_all_files();
    let registration_files: HashSet<_> = paths
        .iter()
        .filter(|file| {
            ctx.read_file(file).is_some_and(|source| {
                [
                    "defineStore",
                    "createStore",
                    "actions",
                    "mutations",
                    "namespaced",
                ]
                .iter()
                .any(|word| source.contains(word))
            })
        })
        .cloned()
        .collect();
    let mut files = HashMap::new();
    for file in paths {
        let nodes = ctx.get_nodes_in_file(&file);
        let Some(language) = nodes.first().map(|n| n.language).filter(|l| {
            matches!(
                l,
                Language::TypeScript | Language::Tsx | Language::JavaScript | Language::Jsx
            )
        }) else {
            continue;
        };
        let Some(source) = ctx.read_file(&file) else {
            continue;
        };
        let imports: Vec<(String, String, String)> = ctx
            .get_import_mappings(&file, language)
            .into_iter()
            .filter_map(|m| {
                let path = m.resolved_path.or_else(|| {
                    crate::resolution::import_resolver::resolve_import_path(
                        &m.source, &file, language, ctx,
                    )
                })?;
                Some((m.local_name, m.exported_name, path))
            })
            .collect();
        if !registration_files.contains(&file)
            && !imports
                .iter()
                .any(|(_, _, path)| registration_files.contains(path))
        {
            continue;
        }
        let Ok(grammar) = Grammar::for_code_language(language) else {
            continue;
        };
        let mut parser = Parser::default();
        parser.set_language(Some(grammar));
        let Some(tree) = parser.parse(&source, None) else {
            continue;
        };
        files.insert(
            file,
            File {
                root: tree.root_node,
                nodes,
                imports,
            },
        );
    }
    let mut stores = Vec::new();
    for (file, data) in &files {
        for decl in declarations(&data.root) {
            let Some(binding) = field(decl, "name") else {
                continue;
            };
            let Some(call) = decl
                .child_for_field_name("value")
                .filter(|n| n.node_type() == "call_expression")
            else {
                continue;
            };
            let Some(factory) = field(call, "function") else {
                continue;
            };
            let package = data
                .root
                .named_children
                .iter()
                .filter(|n| n.node_type() == "import_statement")
                .find_map(|n| {
                    let package = field(n, "source")?.trim_matches(['\'', '"']).to_owned();
                    n.descendants_of_type(&["import_specifier"])
                        .iter()
                        .find_map(|s| {
                            let exported = field(s, "name")?;
                            let local = field(s, "alias").unwrap_or(exported.clone());
                            (local == factory
                                && ((package == "pinia" && exported == "defineStore")
                                    || (package == "vuex" && exported == "createStore")))
                                .then_some(package.clone())
                        })
                });
            let Some(package) = package else { continue };
            let Some(config) = call
                .child_for_field_name("arguments")
                .and_then(|a| a.named_children.iter().find(|n| n.node_type() == "object"))
            else {
                continue;
            };
            if !stable(&data.root, &binding) {
                continue;
            }
            stores.push(Store {
                file: file.clone(),
                binding,
                pinia: package == "pinia",
                members: members(&files, file, config, "", 0),
            });
        }
    }
    let mut edges = Vec::new();
    for (file, data) in &files {
        for call in data.root.descendants_of_type(&["call_expression"]) {
            let Some(function) = call.child_for_field_name("function") else {
                continue;
            };
            if function.node_type() != "member_expression"
                && !(function.node_type() == "identifier"
                    && matches!(function.text().as_str(), "dispatch" | "commit"))
            {
                continue;
            }
            let line = call.start_position.row as u64 + 1;
            let Some(source) = enclosing_fn(&data.nodes, line) else {
                continue;
            };
            // parent() snapshots contain identity only, not child fields; use the full AST node.
            let scope = data
                .root
                .descendants_of_type(&[
                    "function_declaration",
                    "arrow_function",
                    "function_expression",
                    "method_definition",
                ])
                .into_iter()
                .filter(|n| n.start_index <= call.start_index && n.end_index >= call.end_index)
                .min_by_key(|n| n.end_index - n.start_index)
                .unwrap_or(&data.root);
            let mut targets = Vec::new();
            if function.node_type() == "member_expression" {
                let Some(receiver) = function
                    .child_for_field_name("object")
                    .filter(|n| n.node_type() == "identifier")
                    .map(|n| n.text())
                else {
                    continue;
                };
                let Some(method) = field(function, "property") else {
                    continue;
                };
                if !stable(&scope, &receiver) {
                    continue;
                }
                for store in &stores {
                    if store.pinia {
                        // An instance binding must be unique and inside this callable (or module).
                        let decls: Vec<_> = scope
                            .descendants_of_type(&["variable_declarator"])
                            .into_iter()
                            .chain(declarations(&data.root))
                            .filter(|d| field(d, "name").as_deref() == Some(&receiver))
                            .collect();
                        let parameter_shadow =
                            scope.child_for_field_name("parameters").is_some_and(|p| {
                                p.descendants_of_type(&[
                                    "identifier",
                                    "shorthand_property_identifier_pattern",
                                ])
                                .iter()
                                .any(|n| n.text() == receiver)
                            });
                        if decls.len() != 1 || parameter_shadow || shadowed(&scope, &store.binding)
                        {
                            continue;
                        }
                        let Some(value) = decls[0]
                            .child_for_field_name("value")
                            .filter(|n| n.node_type() == "call_expression")
                        else {
                            continue;
                        };
                        let Some(factory) = field(value, "function") else {
                            continue;
                        };
                        if shadowed(&scope, &factory)
                            || !binding_visible(&files, file, &factory, store)
                        {
                            continue;
                        }
                        for (name, kind, target) in &store.members {
                            if name == &method && kind == "actions" {
                                targets.push((target.clone(), "pinia-store", method.clone()));
                            }
                        }
                    } else if matches!(method.as_str(), "dispatch" | "commit")
                        && !shadowed(&scope, &receiver)
                        && binding_visible(&files, file, &receiver, store)
                    {
                        if let Some(arg) = call
                            .child_for_field_name("arguments")
                            .and_then(|a| a.named_children.first())
                            .filter(|n| n.node_type() == "string")
                        {
                            let name = arg.text().trim_matches(['\'', '"']).to_owned();
                            let kind = if method == "dispatch" {
                                "actions"
                            } else {
                                "mutations"
                            };
                            for (key, collection, target) in &store.members {
                                if key == &name && collection == kind {
                                    targets.push((target.clone(), "vuex-dispatch", name.clone()));
                                }
                            }
                        }
                    }
                }
            } else if function.node_type() == "identifier"
                && matches!(function.text().as_str(), "dispatch" | "commit")
            {
                let method = function.text();
                // Bare dispatch/commit only inside an action receiving that context binding.
                let has_context = scope
                    .child_for_field_name("parameters")
                    .and_then(|p| {
                        p.descendants_of_type(&["object_pattern"])
                            .into_iter()
                            .next()
                    })
                    .is_some_and(|p| {
                        p.named_children.iter().any(|n| {
                            n.node_type() == "shorthand_property_identifier_pattern"
                                && n.text() == method
                        })
                    });
                if !has_context
                    || !stable(&scope, &method)
                    || call
                        .child_for_field_name("arguments")
                        .is_some_and(|a| a.named_children.len() > 2)
                {
                    continue;
                }
                if let Some(arg) = call
                    .child_for_field_name("arguments")
                    .and_then(|a| a.named_children.first())
                    .filter(|n| n.node_type() == "string")
                {
                    let name = arg.text().trim_matches(['\'', '"']).to_owned();
                    for store in stores.iter().filter(|s| !s.pinia) {
                        for (action, kind, node) in &store.members {
                            if kind != "actions" || node.id != source.id {
                                continue;
                            }
                            let ns = action
                                .rsplit_once('/')
                                .map(|(ns, _)| format!("{ns}/"))
                                .unwrap_or_default();
                            let key = format!("{ns}{name}");
                            for (target_key, kind, target) in &store.members {
                                if target_key == &key
                                    && kind
                                        == if method == "commit" {
                                            "mutations"
                                        } else {
                                            "actions"
                                        }
                                {
                                    targets.push((target.clone(), "vuex-dispatch", key.clone()));
                                }
                            }
                        }
                    }
                }
            }
            targets.sort_by(|a, b| a.0.id.cmp(&b.0.id));
            targets.dedup_by(|a, b| a.0.id == b.0.id);
            if targets.len() == 1 {
                let (target, mechanism, via) = &targets[0];
                if source.id != target.id {
                    edges.push(edge(
                        &source.id,
                        &target.id,
                        EdgeKind::Calls,
                        Some(line),
                        mechanism,
                        [
                            ("via", json!(via)),
                            ("registeredAt", json!(format!("{file}:{line}"))),
                        ],
                    ));
                }
            }
        }
    }
    edges
}
