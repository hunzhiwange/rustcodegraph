//! Based on upstream 15483713 Vue store extraction, with imported-factory evidence.
//! AST registration evidence for Vue store callable collections.
//! Deliberately produces no dispatch edges; resolution must use this evidence.
use crate::web_tree_sitter::SyntaxNode;

fn field(n: &SyntaxNode, name: &str) -> Option<String> {
    n.child_for_field_name(name).map(|n| n.text())
}
fn key(n: &SyntaxNode) -> Option<String> {
    let k = n
        .child_for_field_name("key")
        .or_else(|| n.child_for_field_name("name"))?;
    matches!(
        k.node_type(),
        "property_identifier" | "identifier" | "string"
    )
    .then(|| k.text().trim_matches(['\'', '"']).to_string())
}
fn resolve_object(root: &SyntaxNode, value: &SyntaxNode) -> Option<SyntaxNode> {
    if value.node_type() == "object" {
        return Some(value.clone());
    }
    if !matches!(
        value.node_type(),
        "identifier" | "shorthand_property_identifier"
    ) {
        return None;
    }
    let name = value.text();
    let matches: Vec<_> = root
        .descendants_of_type(&["variable_declarator"])
        .into_iter()
        .filter(|d| {
            super::rtk::is_module_declaration(d) && field(d, "name").as_deref() == Some(&name)
        })
        .collect();
    if matches.len() != 1 {
        return None;
    }
    if root
        .descendants_of_type(&["assignment_expression", "augmented_assignment_expression"])
        .iter()
        .any(|n| {
            field(n, "left").is_some_and(|s| {
                s == name
                    || s.starts_with(&format!("{name}."))
                    || s.starts_with(&format!("{name}["))
            })
        })
    {
        return None;
    }
    matches[0]
        .child_for_field_name("value")
        .filter(|v| v.node_type() == "object")
        .cloned()
}
pub(crate) fn registrations(root: &SyntaxNode) -> Vec<(String, String, SyntaxNode)> {
    let factories: Vec<_> = root
        .named_children
        .iter()
        .filter(|n| n.node_type() == "import_statement")
        .flat_map(|n| {
            let package = field(n, "source")
                .unwrap_or_default()
                .trim_matches(['\'', '"'])
                .to_owned();
            n.descendants_of_type(&["import_specifier"])
                .into_iter()
                .filter_map(move |s| {
                    let exported = field(s, "name")?;
                    ((package == "pinia" && exported == "defineStore")
                        || (package == "vuex" && exported == "createStore"))
                        .then(|| field(s, "alias").unwrap_or(exported))
                })
        })
        .collect();
    let mut configs = Vec::new();
    for call in root.descendants_of_type(&["call_expression"]) {
        // A nested/local factory name may shadow the imported binding.
        if !call.parent().is_some_and(|p| {
            (p.node_type() == "variable_declarator" && super::rtk::is_module_declaration(&p))
                || p.node_type() == "export_statement"
        }) {
            continue;
        }
        if field(call, "function").is_some_and(|f| factories.contains(&f))
            && let Some(args) = call.child_for_field_name("arguments")
        {
            configs.extend(
                args.named_children
                    .iter()
                    .filter(|n| n.node_type() == "object")
                    .cloned(),
            );
        }
    }
    // Vuex modules register their collections in a literal default-export config.
    for export in root
        .named_children
        .iter()
        .filter(|n| n.node_type() == "export_statement")
    {
        if let Some(config) = export
            .named_children
            .iter()
            .find(|n| n.node_type() == "object")
            && config.named_children.iter().any(|n| {
                key(n).as_deref() == Some("namespaced")
                    && field(n, "value").as_deref() == Some("true")
            })
        {
            configs.push(config.clone());
        }
    }
    let mut result = Vec::new();
    for config in configs {
        for member in &config.named_children {
            let collection = key(member).or_else(|| {
                (member.node_type() == "shorthand_property_identifier").then(|| member.text())
            });
            let Some(collection) =
                collection.filter(|k| matches!(k.as_str(), "actions" | "mutations" | "getters"))
            else {
                continue;
            };
            let value = member.child_for_field_name("value").unwrap_or(member);
            let Some(object) = resolve_object(root, value) else {
                continue;
            };
            for callable in &object.named_children {
                let Some(name) = key(callable) else { continue };
                let function = if callable.node_type() == "method_definition" {
                    Some(callable)
                } else {
                    callable.child_for_field_name("value").filter(|v| {
                        matches!(v.node_type(), "arrow_function" | "function_expression")
                    })
                };
                if let Some(function) = function {
                    result.push((name, collection.clone(), function.clone()));
                }
            }
        }
    }
    result.sort_by_key(|(_, _, n)| n.start_index);
    result.dedup_by_key(|(_, _, n)| n.start_index);
    result
}
