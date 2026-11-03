//! Candidate lookup indexes must preserve contextual target selection.
use rustcodegraph::types::{EdgeKind, NodeKind};
use rustcodegraph::{CodeGraph, IndexOptions};
use std::path::PathBuf;

struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn fixture(files: &[(&str, &str)]) -> (Fixture, CodeGraph) {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let root = Fixture(std::env::temp_dir().join(format!(
        "rcg-069-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    )));
    std::fs::create_dir_all(&root.0).unwrap();
    for (path, text) in files {
        std::fs::write(root.0.join(path), text).unwrap();
    }
    let mut graph = CodeGraph::init_sync(&root.0).unwrap();
    let result = graph.index_all(IndexOptions::default());
    assert!(result.success, "{:?}", result.errors);
    (root, graph)
}

#[test]
fn nix_same_name_candidates_keep_same_file_target() {
    let (_root, mut graph) = fixture(&[
        ("a.nix", "{ helper = x: x; start = x: helper x; }"),
        ("b.nix", "{ helper = x: x; start = x: helper x; }"),
    ]);
    for file in ["a.nix", "b.nix"] {
        let nodes = graph.get_nodes_in_file(file);
        let start = nodes.iter().find(|n| n.name == "start").unwrap();
        let helper = nodes.iter().find(|n| n.name == "helper").unwrap();
        let calls: Vec<_> = graph
            .get_outgoing_edges(&start.id)
            .into_iter()
            .filter(|e| e.kind == EdgeKind::Calls)
            .collect();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].target, helper.id);
    }
}

#[test]
fn typed_field_member_call_keeps_receiver_type_among_duplicate_methods() {
    let (_root, mut graph) = fixture(&[(
        "main.ts",
        "class Client { send() { return 1; } } class Other { send() { return 2; } } class Service { client: Client; run() { return this.client.send(); } }",
    )]);
    let nodes = graph.get_nodes_in_file("main.ts");
    let run = nodes
        .iter()
        .find(|n| n.name == "run" && n.kind == NodeKind::Method)
        .unwrap();
    let client_send = nodes
        .iter()
        .find(|n| n.name == "send" && n.qualified_name.contains("Client"))
        .unwrap();
    assert!(
        graph
            .get_outgoing_edges(&run.id)
            .iter()
            .any(|e| e.kind == EdgeKind::Calls && e.target == client_send.id)
    );
}
