use rustcodegraph::extraction::index::extract_from_source;
use rustcodegraph::types::ReferenceKind;

#[test]
fn case_065_imports_calls_and_scope() {
    let source = "let run = x: let inner = y: lib.map f y; in inner (import (\"./模块.nix\") {}); in { pkg = pkgs.callPackage ../pkg {}; multi = f 1 2; builtin = builtins.import \"./other.nix\"; }";
    let result = extract_from_source("default.nix", source, None, None);
    let refs = &result.unresolved_references;
    for (name, kind, owner) in [
        ("lib.map", ReferenceKind::Calls, "inner"),
        ("inner", ReferenceKind::Calls, "run"),
        ("./模块.nix", ReferenceKind::Imports, "run"),
        ("pkgs.callPackage", ReferenceKind::Calls, "default.nix"),
        ("../pkg", ReferenceKind::Imports, "default.nix"),
        ("f", ReferenceKind::Calls, "default.nix"),
        ("./other.nix", ReferenceKind::Imports, "default.nix"),
    ] {
        let matches: Vec<_> = refs
            .iter()
            .filter(|r| r.reference_name == name && r.reference_kind == kind)
            .collect();
        assert_eq!(matches.len(), 1, "{name}: {refs:?}");
        assert_eq!(
            matches[0].from_node_id,
            result.nodes.iter().find(|n| n.name == owner).unwrap().id
        );
    }
    assert_eq!(refs.len(), 7, "{refs:?}");
    assert!(
        result
            .edges
            .iter()
            .all(|e| e.kind != rustcodegraph::types::EdgeKind::Calls)
    );
}

#[test]
fn case_065_dynamic_and_text_are_not_imports() {
    let source = r#"{ a = import path; b = import ./foo/${name}; c = callPackage pkgPath {}; d = "import ./fake; f x"; # import ./comment
 e = import <nixpkgs>; f = pkgs.${method} 1; }"#;
    let result = extract_from_source("default.nix", source, None, None);
    assert!(
        result
            .unresolved_references
            .iter()
            .all(|r| r.reference_kind != ReferenceKind::Imports)
    );
    assert_eq!(
        result
            .unresolved_references
            .iter()
            .map(|r| r.reference_name.as_str())
            .collect::<Vec<_>>(),
        vec!["callPackage"]
    );
}

#[test]
fn case_065_multibyte_positions() {
    let source = "{ text = \"中文\"; run = x: lib.f x; }";
    let result = extract_from_source("default.nix", source, None, None);
    let reference = result
        .unresolved_references
        .iter()
        .find(|r| r.reference_name == "lib.f")
        .unwrap();
    assert_eq!(reference.line, 1);
    assert_eq!(reference.column, source.find("lib.f").unwrap() as u64);
}

#[test]
fn case_065_sqlite_index_preserves_import_nodes() {
    let path = std::env::temp_dir().join(format!("rustcodegraph-065-{}", std::process::id()));
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(path.clone());
    std::fs::create_dir_all(&path).unwrap();
    std::fs::write(
        path.join("default.nix"),
        "{ pkg = pkgs.callPackage ./pkg.nix {}; }",
    )
    .unwrap();
    let mut graph = rustcodegraph::CodeGraph::init_sync(&path).unwrap();
    for _ in 0..2 {
        let result = graph.index_all(rustcodegraph::IndexOptions::default());
        assert!(result.success, "{:?}", result.errors);
        let nodes = graph.get_nodes_in_file("default.nix");
        assert_eq!(
            nodes
                .iter()
                .filter(
                    |n| n.kind == rustcodegraph::types::NodeKind::Import && n.name == "./pkg.nix"
                )
                .count(),
            1
        );
    }
    graph.close();
}
