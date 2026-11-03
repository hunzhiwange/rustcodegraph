//! A static Nix module-loading flow; no option-system inference is required.
use rustcodegraph::types::{EdgeKind, NodeKind};
use rustcodegraph::{CodeGraph, IndexOptions};

fn check_flow(expression: &str, expected: bool) {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let dir = Cleanup(std::env::temp_dir().join(format!(
        "rustcodegraph-067-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    )));
    std::fs::create_dir_all(dir.0.join("module")).unwrap();
    std::fs::create_dir_all(dir.0.join("unrelated")).unwrap();
    std::fs::write(
        dir.0.join("main.nix"),
        format!("{{ load = arg: {expression}; }}"),
    )
    .unwrap();
    std::fs::write(dir.0.join("module/default.nix"), "{ finish = x: x; }").unwrap();
    std::fs::write(dir.0.join("unrelated/default.nix"), "{ finish = x: x; }").unwrap();
    let mut graph = CodeGraph::init_sync(&dir.0).unwrap();
    let result = graph.index_all(IndexOptions::default());
    assert!(result.success, "{:?}", result.errors);
    let source = graph
        .get_nodes_in_file("main.nix")
        .into_iter()
        .find(|n| n.name == "load" && n.kind == NodeKind::Function)
        .unwrap();
    let targets = graph.get_nodes_in_file("module/default.nix");
    let module = targets.iter().find(|n| n.kind == NodeKind::File).unwrap();
    let finish = targets.iter().find(|n| n.name == "finish").unwrap();
    let imports: Vec<_> = graph
        .get_outgoing_edges(&source.id)
        .into_iter()
        .filter(|e| e.kind == EdgeKind::Imports && e.target.starts_with("file:"))
        .collect();
    assert_eq!(imports.iter().any(|e| e.target == module.id), expected);
    assert_eq!(
        imports.len(),
        usize::from(expected),
        "no unrelated basename fallback"
    );
    if expected {
        assert!(
            imports.iter().all(|e| e.provenance.is_none()),
            "static imports must not become heuristic"
        );
        assert!(
            graph
                .get_outgoing_edges(&module.id)
                .iter()
                .any(|e| e.kind == EdgeKind::Contains && e.target == finish.id),
            "the module-loading flow must reach the module's named declaration"
        );
    }
    graph.close();
}

#[test]
fn case_067_static_import_module_flow() {
    check_flow("import ./module", true);
}
#[test]
fn case_067_static_callpackage_module_flow() {
    check_flow("pkgs.callPackage ./module {}", true);
}
#[test]
fn case_067_missing_module_does_not_match_other_basename() {
    check_flow("import ./missing/default.nix", false);
}
#[test]
fn case_067_dynamic_module_stays_unresolved() {
    check_flow("pkgs.callPackage modulePath {}", false);
}
