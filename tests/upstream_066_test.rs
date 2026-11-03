use rustcodegraph::types::{EdgeKind, NodeKind};
use rustcodegraph::{CodeGraph, IndexOptions};

fn imports(source: &str, files: &[(&str, &str)]) -> Vec<String> {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let dir = Cleanup(std::env::temp_dir().join(format!(
        "rustcodegraph-066-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    )));
    for (path, content) in std::iter::once(("app/main.nix", source)).chain(files.iter().copied()) {
        let path = dir.0.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }
    let mut graph = CodeGraph::init_sync(&dir.0).unwrap();
    let result = graph.index_all(IndexOptions::default());
    assert!(result.success, "{:?}", result.errors);
    let nodes = graph.get_nodes_in_file("app/main.nix");
    let owner = nodes.iter().find(|n| n.kind == NodeKind::File).unwrap();
    let edges = graph.get_outgoing_edges(&owner.id);
    let mut targets = Vec::new();
    for (path, _) in files {
        for node in graph.get_nodes_in_file(path) {
            if edges
                .iter()
                .any(|e| e.kind == EdgeKind::Imports && e.target == node.id)
            {
                assert_eq!(node.kind, NodeKind::File, "imports must target files");
                targets.push(node.file_path);
            }
        }
    }
    targets.sort();
    graph.close();
    targets
}

#[test]
fn case_066_relative_paths_disambiguate_same_basename() {
    assert_eq!(
        imports(
            "{ a = import ./pkg.nix; b = builtins.import ../shared/pkg.nix; }",
            &[
                ("app/pkg.nix", "{}"),
                ("shared/pkg.nix", "{}"),
                ("other/pkg.nix", "{}")
            ]
        ),
        vec!["app/pkg.nix", "shared/pkg.nix"]
    );
}

#[test]
fn case_066_directory_import_and_callpackage() {
    assert_eq!(
        imports(
            "{ a = import ./module; b = pkgs.callPackage ../packages {}; }",
            &[
                ("app/module/default.nix", "{}"),
                ("packages/default.nix", "{}"),
                ("other/default.nix", "{}")
            ]
        ),
        vec!["app/module/default.nix", "packages/default.nix"]
    );
}

#[test]
fn case_066_missing_path_never_falls_back_to_basename() {
    assert!(
        imports(
            "{ a = import ./missing/pkg.nix; }",
            &[("other/pkg.nix", "{}")]
        )
        .is_empty()
    );
}

#[test]
fn case_066_external_dynamic_and_unrelated_symbols_stay_unresolved() {
    assert!(
        imports(
            "{ a = import <nixpkgs>; b = import path; c = import ./missing; }",
            &[
                ("other/missing.nix", "{}"),
                ("other/defs.nix", "{ missing = x: x; path = x: x; }")
            ]
        )
        .is_empty()
    );
}
