//! Literal, same-module RTK Query registration evidence shared by extraction and resolution.
use crate::web_tree_sitter::SyntaxNode;

pub(crate) struct Registration {
    pub hook: String,
    pub binding: SyntaxNode,
    pub endpoint: String,
    pub callback: SyntaxNode,
}
fn field(n: &SyntaxNode, name: &str) -> Option<String> {
    n.child_for_field_name(name).map(|n| n.text())
}
pub(crate) fn is_module_declaration(n: &SyntaxNode) -> bool {
    let Some(parent) = n.parent() else {
        return false;
    };
    let Some(parent) = parent.parent() else {
        return false;
    };
    parent.node_type() == "program" || parent.node_type() == "export_statement"
}
fn declarations(root: &SyntaxNode) -> Vec<&SyntaxNode> {
    root.descendants_of_type(&["variable_declarator"])
        .into_iter()
        .filter(|n| is_module_declaration(n))
        .collect()
}
pub(crate) fn registrations(root: &SyntaxNode) -> Vec<Registration> {
    let factories: Vec<_> = root
        .named_children
        .iter()
        .filter(|n| n.node_type() == "import_statement")
        .filter(|n| {
            field(n, "source")
                .is_some_and(|s| s.trim_matches(['\'', '"']) == "@reduxjs/toolkit/query/react")
        })
        .flat_map(|n| n.descendants_of_type(&["import_specifier"]))
        .filter(|n| field(n, "name").as_deref() == Some("createApi"))
        .filter_map(|n| field(n, "alias").or_else(|| field(n, "name")))
        .collect();
    let decls = declarations(root);
    let mut result = Vec::new();
    for api in &decls {
        let Some(api_name) = field(api, "name") else {
            continue;
        };
        if decls
            .iter()
            .filter(|d| field(d, "name").as_deref() == Some(&api_name))
            .count()
            != 1
        {
            continue;
        }
        let Some(value) = api
            .child_for_field_name("value")
            .filter(|n| n.node_type() == "call_expression")
        else {
            continue;
        };
        if !field(value, "function").is_some_and(|f| factories.contains(&f)) {
            continue;
        }
        // Reassignment makes the exported binding's runtime API uncertain.
        if root
            .descendants_of_type(&["assignment_expression", "augmented_assignment_expression"])
            .iter()
            .any(|n| {
                field(n, "left").is_some_and(|left| {
                    left == api_name
                        || left.starts_with(&format!("{api_name}."))
                        || left.starts_with(&format!("{api_name}["))
                })
            })
        {
            continue;
        }
        let Some(config) = value
            .child_for_field_name("arguments")
            .and_then(|a| a.named_children.first())
            .filter(|n| n.node_type() == "object")
        else {
            continue;
        };
        if config
            .named_children
            .iter()
            .any(|n| n.node_type() == "spread_element")
        {
            continue;
        }
        if config
            .named_children
            .iter()
            .filter(|n| {
                field(n, "key").or_else(|| field(n, "name")).as_deref() == Some("endpoints")
            })
            .count()
            != 1
        {
            continue;
        }
        for endpoints in &config.named_children {
            if field(endpoints, "key")
                .or_else(|| field(endpoints, "name"))
                .as_deref()
                != Some("endpoints")
            {
                continue;
            }
            let function = endpoints.child_for_field_name("value").unwrap_or(endpoints);
            let Some(params) = function
                .child_for_field_name("parameters")
                .or_else(|| function.child_for_field_name("parameter"))
            else {
                continue;
            };
            let builder = if params.node_type() == "identifier" {
                params.text()
            } else {
                let Some(p) = params.named_children.first().and_then(|p| {
                    if p.node_type() == "identifier" {
                        Some(p)
                    } else if p.node_type() == "required_parameter" {
                        p.child_for_field_name("pattern")
                            .filter(|n| n.node_type() == "identifier")
                    } else {
                        None
                    }
                }) else {
                    continue;
                };
                p.text()
            };
            if function
                .descendants_of_type(&["assignment_expression", "augmented_assignment_expression"])
                .iter()
                .any(|n| field(n, "left").as_deref() == Some(&builder))
            {
                continue;
            }
            let Some(object) = super::function_returned_object(function) else {
                continue;
            };
            if object
                .named_children
                .iter()
                .any(|n| n.node_type() == "spread_element")
            {
                continue;
            }
            for endpoint in &object.named_children {
                let Some(name) = endpoint
                    .child_for_field_name("key")
                    .filter(|n| n.node_type() == "property_identifier")
                    .map(|n| n.text())
                else {
                    continue;
                };
                if object
                    .named_children
                    .iter()
                    .filter(|n| field(n, "key").as_deref() == Some(&name))
                    .count()
                    != 1
                {
                    continue;
                }
                let Some(call) = endpoint
                    .child_for_field_name("value")
                    .filter(|n| n.node_type() == "call_expression")
                else {
                    continue;
                };
                let Some(member) = call
                    .child_for_field_name("function")
                    .filter(|n| n.node_type() == "member_expression")
                else {
                    continue;
                };
                if field(member, "object").as_deref() != Some(&builder) {
                    continue;
                }
                let suffix = match field(member, "property").as_deref() {
                    Some("query") => "Query",
                    Some("mutation") => "Mutation",
                    _ => continue,
                };
                let Some(options) = call
                    .child_for_field_name("arguments")
                    .and_then(|a| a.named_children.first())
                    .filter(|n| n.node_type() == "object")
                else {
                    continue;
                };
                let callbacks: Vec<_> = options
                    .named_children
                    .iter()
                    .filter_map(|p| {
                        let key = field(p, "key").or_else(|| field(p, "name"))?;
                        if key != "queryFn" && key != "query" {
                            return None;
                        }
                        let callback = p.child_for_field_name("value").unwrap_or(p);
                        matches!(
                            callback.node_type(),
                            "arrow_function" | "function_expression" | "method_definition"
                        )
                        .then_some(callback)
                    })
                    .collect();
                if callbacks.len() != 1 {
                    continue;
                }
                let mut characters = name.chars();
                let Some(first) = characters.next() else {
                    continue;
                };
                let capitalized = format!("{}{}", first.to_uppercase(), characters.as_str());
                for binding in &decls {
                    if field(binding, "value").as_deref() != Some(&api_name) {
                        continue;
                    }
                    let Some(pattern) = binding
                        .child_for_field_name("name")
                        .filter(|n| n.node_type() == "object_pattern")
                    else {
                        continue;
                    };
                    for property in &pattern.named_children {
                        let (exported, local) =
                            if property.node_type() == "shorthand_property_identifier_pattern" {
                                (property.text(), property.text())
                            } else if property.node_type() == "pair_pattern" {
                                let (Some(key), Some(value)) = (
                                    field(property, "key"),
                                    property
                                        .child_for_field_name("value")
                                        .filter(|n| n.node_type() == "identifier")
                                        .map(|n| n.text()),
                                ) else {
                                    continue;
                                };
                                (key, value)
                            } else {
                                continue;
                            };
                        if exported != format!("use{capitalized}{suffix}")
                            && !(suffix == "Query"
                                && exported == format!("useLazy{capitalized}Query"))
                        {
                            continue;
                        }
                        result.push(Registration {
                            hook: local,
                            binding: (*binding).clone(),
                            endpoint: name.clone(),
                            callback: callbacks[0].clone(),
                        });
                    }
                }
            }
        }
    }
    // Node IDs use file/name/line. Abstain when same-line APIs would collide.
    let mut positions = std::collections::HashMap::<_, std::collections::HashSet<_>>::new();
    for registration in &result {
        positions
            .entry((
                registration.endpoint.clone(),
                registration.callback.start_position.row,
            ))
            .or_default()
            .insert(registration.callback.start_index);
    }
    result.retain(|registration| {
        positions[&(
            registration.endpoint.clone(),
            registration.callback.start_position.row,
        )]
            .len()
            == 1
    });
    result
}
