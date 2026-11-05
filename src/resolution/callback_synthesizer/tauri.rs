//! Evidence-bound Tauri command IPC (upstream tauri-synthesizer.ts @15483713).
use super::common::{edge, enclosing_fn};
use crate::{
    resolution::types::ResolutionContext,
    types::{Edge, EdgeKind, Language, Node, NodeKind},
    web_tree_sitter::{Language as Grammar, Parser, SyntaxNode},
};
use serde_json::json;
use std::collections::{BTreeMap, HashSet};

fn parse(source: &str, lang: Language) -> Option<SyntaxNode> {
    let mut parser = Parser::default();
    parser.set_language(Some(Grammar::for_code_language(lang).ok()?));
    Some(parser.parse(source, None)?.root_node)
}
fn field(n: &SyntaxNode, key: &str) -> Option<String> {
    n.child_for_field_name(key).map(|n| n.text())
}
fn app(file: &str) -> Option<String> {
    let parts: Vec<_> = file.split('/').collect();
    let at = parts.iter().position(|s| *s == "src-tauri")?;
    Some(parts[..at].join("/"))
}
fn within(file: &str, root: &str) -> bool {
    root.is_empty() || file.starts_with(&format!("{root}/"))
}
fn declared_command(f: &SyntaxNode) -> bool {
    let mut sibling = f.previous_named_sibling();
    while let Some(n) = sibling {
        if n.node_type() == "attribute_item" {
            let text: String = n.text().chars().filter(|c| !c.is_whitespace()).collect();
            if text == "#[tauri::command]" || text.starts_with("#[tauri::command(") {
                return true;
            }
        } else if !matches!(n.node_type(), "line_comment" | "block_comment") {
            break;
        }
        sibling = n.previous_named_sibling();
    }
    false
}
// Reject any shadow/reassignment of the imported name. This deliberately errs
// on the side of silence even when the conflicting binding is in another scope.
fn rebound(root: &SyntaxNode, name: &str) -> bool {
    root.descendants_of_type(&[
        "variable_declarator",
        "function_declaration",
        "class_declaration",
        "required_parameter",
        "optional_parameter",
        "assignment_expression",
        "augmented_assignment_expression",
        "update_expression",
        "catch_clause",
        "arrow_function",
        "formal_parameters",
    ])
    .iter()
    .any(|n| {
        let target = n
            .child_for_field_name("name")
            .or_else(|| n.child_for_field_name("pattern"))
            .or_else(|| n.child_for_field_name("left"))
            .or_else(|| n.child_for_field_name("argument"))
            .or_else(|| n.child_for_field_name("parameter"));
        target.is_some_and(|t| {
            t.text() == name
                || t.descendants_of_type(&["identifier", "shorthand_property_identifier_pattern"])
                    .iter()
                    .any(|i| i.text() == name)
        }) || (n.node_type() == "formal_parameters"
            && n.named_children
                .iter()
                .any(|c| c.node_type() == "identifier" && c.text() == name))
    })
}
struct Registration {
    app: String,
    name: String,
    target: Node,
    at: String,
}
pub(super) fn command_edges(ctx: &mut dyn ResolutionContext) -> Vec<Edge> {
    let files = ctx.get_all_files();
    let mut rust = BTreeMap::new();
    for file in &files {
        if !file.ends_with(".rs") || app(file).is_none() {
            continue;
        }
        let Some(source) = ctx.read_file(file) else {
            continue;
        };
        if let Some(root) = parse(&source, Language::Rust) {
            rust.insert(file.clone(), (root, ctx.get_nodes_in_file(file)));
        }
    }
    let mut registrations = Vec::new();
    for (file, (root, _)) in &rust {
        for m in root.descendants_of_type(&["macro_invocation"]) {
            if m.has_error || field(m, "macro").as_deref() != Some("tauri::generate_handler") {
                continue;
            }
            // Inline modules change Rust's name lookup. Until those paths are
            // resolved, never attach their registration to a file-level namesake.
            if root
                .descendants_of_type(&["mod_item", "impl_item", "trait_item"])
                .iter()
                .any(|scope| scope.start_index <= m.start_index && scope.end_index >= m.end_index)
            {
                continue;
            }
            let Some(tokens) = m
                .named_children
                .iter()
                .find(|n| n.node_type() == "token_tree")
            else {
                continue;
            };
            // Inspect AST tokens, never comments or the contents of strings.
            let children: Vec<_> = tokens
                .children
                .iter()
                .filter(|n| !matches!(n.node_type(), "line_comment" | "block_comment"))
                .collect();
            if children.len() < 2 {
                continue;
            }
            for entry in children[1..children.len() - 1].split(|n| n.text() == ",") {
                if entry.is_empty()
                    || entry
                        .iter()
                        .any(|n| !matches!(n.node_type(), "identifier" | "::"))
                {
                    continue;
                }
                let path = entry.iter().map(|n| n.text()).collect::<String>();
                let parts: Vec<_> = path.split("::").collect();
                if parts.iter().any(|s| s.is_empty()) {
                    continue;
                }
                let name = parts[parts.len() - 1];
                let dir = file.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
                let paths = if parts.len() == 1 {
                    // A local declaration can hide the module-level command.
                    if root
                        .descendants_of_type(&["function_item"])
                        .iter()
                        .filter(|f| field(f, "name").as_deref() == Some(name))
                        .count()
                        != 1
                    {
                        continue;
                    }
                    vec![file.clone()]
                } else {
                    let base = if file.ends_with("/lib.rs")
                        || file.ends_with("/main.rs")
                        || file.ends_with("/mod.rs")
                    {
                        dir.to_owned()
                    } else {
                        file.trim_end_matches(".rs").to_owned()
                    };
                    let module = parts[..parts.len() - 1].join("/");
                    vec![
                        format!("{base}/{module}.rs"),
                        format!("{base}/{module}/mod.rs"),
                    ]
                };
                let mut candidates = Vec::new();
                for target_file in paths {
                    let Some((tree, nodes)) = rust.get(&target_file) else {
                        continue;
                    };
                    for f in &tree.named_children {
                        if f.node_type() != "function_item"
                            || field(f, "name").as_deref() != Some(name)
                            || !declared_command(f)
                        {
                            continue;
                        }
                        candidates.extend(
                            nodes
                                .iter()
                                .filter(|n| {
                                    n.kind == NodeKind::Function
                                        && n.name == name
                                        && n.start_line <= f.start_position.row as u64 + 1
                                        && n.end_line >= f.end_position.row as u64 + 1
                                })
                                .cloned(),
                        );
                    }
                }
                if candidates.len() == 1 {
                    registrations.push(Registration {
                        app: app(file).unwrap(),
                        name: name.to_owned(),
                        target: candidates.remove(0),
                        at: format!("{file}:{}", entry[0].start_position.row + 1),
                    });
                }
            }
        }
    }
    if registrations.is_empty() {
        return Vec::new();
    }
    // An app with no registrations still owns its frontend: never fall back
    // across its boundary to an enclosing app's command table.
    let apps: HashSet<_> = rust.keys().filter_map(|file| app(file)).collect();
    let mut out = Vec::new();
    for file in &files {
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
        let imports = ctx.get_import_mappings(file, lang);
        if !imports.iter().any(|i| {
            [
                "@tauri-apps/api/core",
                "@tauri-apps/api/tauri",
                "@tauri-apps/api",
            ]
            .contains(&i.source.as_str())
        }) {
            continue;
        }
        let Some(source) = ctx.read_file(file) else {
            continue;
        };
        let Some(root) = parse(&source, lang) else {
            continue;
        };
        let bindings: HashSet<_> = imports
            .iter()
            .filter(|i| {
                [
                    "@tauri-apps/api/core",
                    "@tauri-apps/api/tauri",
                    "@tauri-apps/api",
                ]
                .contains(&i.source.as_str())
                    && (i.exported_name == "invoke" || i.is_namespace)
                    && !i.is_default
                    && !rebound(&root, &i.local_name)
                    && imports
                        .iter()
                        .filter(|j| j.local_name == i.local_name)
                        .count()
                        == 1
            })
            .map(|i| {
                if i.is_namespace {
                    format!("{}.invoke", i.local_name)
                } else {
                    i.local_name.clone()
                }
            })
            .collect();
        if bindings.is_empty() {
            continue;
        }
        let nodes = ctx.get_nodes_in_file(file);
        let scope = apps
            .iter()
            .filter(|root| within(file, root))
            .map(|root| root.len())
            .max();
        for call in root.descendants_of_type(&["call_expression"]) {
            if call.has_error || !field(call, "function").is_some_and(|f| bindings.contains(&f)) {
                continue;
            }
            let Some(arg) = call
                .child_for_field_name("arguments")
                .and_then(|a| a.named_children.iter().find(|n| n.node_type() != "comment"))
            else {
                continue;
            };
            if arg.node_type() != "string" {
                continue;
            }
            let text = arg.text();
            let Some(command) = text
                .get(1..text.len().saturating_sub(1))
                .filter(|s| !s.is_empty() && !s.contains('\\'))
            else {
                continue;
            };
            let candidates: Vec<_> = registrations
                .iter()
                .filter(|r| Some(r.app.len()) == scope && within(file, &r.app) && r.name == command)
                .collect();
            let targets: HashSet<_> = candidates.iter().map(|r| &r.target.id).collect();
            if targets.len() != 1 {
                continue;
            }
            let r = candidates[0];
            let line = call.start_position.row as u64 + 1;
            let Some(caller) = enclosing_fn(&nodes, line) else {
                continue;
            };
            out.push(edge(
                &caller.id,
                &r.target.id,
                EdgeKind::Calls,
                Some(line),
                "tauri-command",
                [
                    ("command", json!(command)),
                    ("registeredAt", json!(r.at)),
                    ("tier", json!("client→server")),
                ],
            ));
        }
    }
    out
}
