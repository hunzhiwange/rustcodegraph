use rustcodegraph::extraction::tree_sitter::extract_from_source;
use rustcodegraph::types::{Language, NodeKind};

#[test]
fn rust_chained_router_extraction_does_not_clone_ast_subtrees_exponentially() {
    let mut source = String::from(
        r#"
use axum::routing::post;
use axum::Router;

pub(crate) fn router<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    Router::new()
"#,
    );
    for index in 0..24 {
        source.push_str(&format!(
            "        .route(\"/task-pool/{index}\", post(handle_{index}))\n"
        ));
    }
    source.push_str("}\n\n");
    for index in 0..24 {
        source.push_str(&format!(
            r#"
async fn handle_{index}() -> Result<(), ApiError> {{
    Ok(())
}}
"#
        ));
    }

    let result = extract_from_source(
        "crates/app/src/task_pool.rs",
        &source,
        Some(Language::Rust),
        None,
    );

    assert!(result.errors.is_empty(), "{:?}", result.errors);
    let functions = result
        .nodes
        .iter()
        .filter(|node| node.kind == NodeKind::Function)
        .count();
    assert!(
        functions >= 25,
        "expected router plus handlers, got {functions} functions"
    );
}

#[test]
fn cloned_nodes_preserve_the_full_ancestor_chain_and_text() {
    use rustcodegraph::web_tree_sitter::{Language as Grammar, Parser};
    let mut parser = Parser::default();
    parser.set_language(Some(Grammar::load("typescript").unwrap()));
    let source = "export class Example { run() { if (true) { const value = 1; return value; } } }";
    let tree = parser.parse(source, None).unwrap();
    let mut pending = vec![tree.root_node.clone()];
    let mut leaf = None;
    while let Some(node) = pending.pop() {
        if node.node_type() == "identifier" && node.text() == "value" {
            leaf = Some(node.clone());
            break;
        }
        pending.extend(node.named_children());
    }
    let mut cursor = leaf.unwrap().parent();
    let mut ancestors = Vec::new();
    while let Some(node) = cursor {
        assert_eq!(node.text(), &source[node.start_index..node.end_index]);
        ancestors.push(node.node_type.clone());
        cursor = node.parent();
    }
    assert!(
        ancestors.iter().any(|kind| kind == "method_definition"),
        "{ancestors:?}"
    );
    assert!(
        ancestors.iter().any(|kind| kind == "export_statement"),
        "{ancestors:?}"
    );
    assert_eq!(ancestors.last().unwrap(), "program");
}
