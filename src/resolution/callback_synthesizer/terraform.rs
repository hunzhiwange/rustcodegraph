//! Conservative cloudposse/atmos component-state reachability.
use super::common::edge;
use crate::{
    extraction::{index::extract_from_source, languages::index::terraform::literal_string},
    resolution::types::ResolutionContext,
    types::{Edge, EdgeKind, Language, Node, NodeKind},
    web_tree_sitter::{Language as Grammar, Parser},
};
use serde_json::json;
use std::{collections::HashMap, path::Path};

pub(super) fn terraform_remote_state_edges(ctx: &mut dyn ResolutionContext) -> Vec<Edge> {
    let files: Vec<_> = ctx
        .get_all_files()
        .into_iter()
        .filter(|f| {
            matches!(
                Path::new(f).extension().and_then(|s| s.to_str()),
                Some("tf" | "tofu")
            )
        })
        .collect();
    if files.is_empty() {
        return Vec::new();
    }
    let mut nodes = Vec::new();
    let mut attributes: HashMap<String, HashMap<String, String>> = HashMap::new();
    let mut references = Vec::new();
    let Ok(grammar) = Grammar::for_code_language(Language::Terraform) else {
        return Vec::new();
    };
    let mut parser = Parser::default();
    parser.set_language(Some(grammar));
    for file in files {
        let file_nodes = ctx.get_nodes_in_file(&file);
        let Some(source) = ctx.read_file(&file) else {
            continue;
        };
        let Some(tree) = parser.parse(&source, None) else {
            continue;
        };
        for body in &tree.root_node.named_children {
            for block in body
                .named_children
                .iter()
                .filter(|n| n.node_type() == "block")
            {
                let Some(node) = file_nodes.iter().find(|n| {
                    n.start_line == block.start_position.row as u64 + 1
                        && n.start_column == block.start_position.column as u64
                        && n.kind != NodeKind::File
                }) else {
                    continue;
                };
                let Some(body) = block
                    .named_children
                    .iter()
                    .find(|n| n.node_type() == "body")
                else {
                    continue;
                };
                let mut attrs = HashMap::new();
                for attr in body
                    .named_children
                    .iter()
                    .filter(|n| n.node_type() == "attribute")
                {
                    let Some(key) = attr
                        .named_children
                        .iter()
                        .find(|n| n.node_type() == "identifier")
                    else {
                        continue;
                    };
                    let Some(value) = attr
                        .named_children
                        .iter()
                        .find(|n| n.node_type() == "expression")
                    else {
                        continue;
                    };
                    let key = source[key.start_index..key.end_index].to_owned();
                    // Duplicate attributes are invalid configuration; refuse the declaration.
                    if attrs
                        .insert(key, source[value.start_index..value.end_index].to_owned())
                        .is_some()
                    {
                        attrs.clear();
                        break;
                    }
                }
                attributes.insert(node.id.clone(), attrs);
            }
        }
        references.extend(
            extract_from_source(&file, &source, None, None)
                .unresolved_references
                .into_iter()
                .map(|r| (file.clone(), r)),
        );
        nodes.extend(
            file_nodes
                .into_iter()
                .filter(|n| n.language == Language::Terraform),
        );
    }
    let mut edges = Vec::new();
    for (file, reference) in references {
        let Some((module, output)) = reference.reference_name.split_once(":remote-output.") else {
            continue;
        };
        let declarations: Vec<_> = nodes
            .iter()
            .filter(|n| {
                n.kind == NodeKind::Module
                    && n.qualified_name == module
                    && parent(n) == Path::new(&file).parent()
            })
            .collect();
        let [decl] = declarations.as_slice() else {
            continue;
        };
        let Some(attrs) = attributes.get(&decl.id) else {
            continue;
        };
        let Some(source) = attrs.get("source").and_then(|s| literal_string(s)) else {
            continue;
        };
        // Exact known module identity, not any unrelated module named remote-state.
        if source != "cloudposse/stack-config/yaml//modules/remote-state" {
            continue;
        }
        let Some(component_expr) = attrs.get("component") else {
            continue;
        };
        let component = literal_string(component_expr).or_else(|| {
            let name = component_expr.trim().strip_prefix("var.")?;
            if name.is_empty()
                || !name
                    .chars()
                    .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
            {
                return None;
            }
            let vars: Vec<_> = nodes
                .iter()
                .filter(|n| n.qualified_name == format!("var.{name}") && parent(n) == parent(decl))
                .collect();
            let [var] = vars.as_slice() else { return None };
            attributes
                .get(&var.id)?
                .get("default")
                .and_then(|s| literal_string(s))
        });
        let Some(component) = component.filter(|c| {
            !c.is_empty()
                && !c.starts_with('/')
                && !c.split('/').any(|s| s.is_empty() || s == "." || s == "..")
        }) else {
            continue;
        };
        let targets: Vec<_> = nodes
            .iter()
            .filter(|n| {
                n.qualified_name == format!("output.{output}")
                    && parent(n).is_some_and(|p| {
                        p == Path::new(&component) || p.ends_with(Path::new(&component))
                    })
            })
            .collect();
        let [target] = targets.as_slice() else {
            continue;
        };
        edges.push(edge(
            &reference.from_node_id,
            &target.id,
            EdgeKind::References,
            Some(reference.line),
            "terraform-remote-state",
            [(
                "registeredAt",
                json!(format!("{}:{}", decl.file_path, decl.start_line)),
            )],
        ));
    }
    edges
}

fn parent(node: &Node) -> Option<&Path> {
    Path::new(&node.file_path).parent()
}
