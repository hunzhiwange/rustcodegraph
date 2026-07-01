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
