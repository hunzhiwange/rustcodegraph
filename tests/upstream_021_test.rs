use rustcodegraph::types::{EdgeKind, NodeKind};
use rustcodegraph::{CodeGraph, IndexOptions};

#[test]
fn case_021_cfg_candidates() {
    let dir = std::env::temp_dir().join(format!("rcg21-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("src")).unwrap();
    for (file, text) in [
        (
            "lib.rs",
            "#[cfg(unix)]\n#[path = \"unix.rs\"]\nmod imp;\n#[cfg(windows)]\n#[path = \"windows.rs\"]\nmod imp;\nuse crate::imp::unique;\nuse crate::imp::shared;\nuse crate::imp::missing;\npub fn run() { unique(); shared(); missing(); }",
        ),
        ("unix.rs", "pub fn unique() {}\npub fn shared() {}"),
        ("windows.rs", "pub fn shared() {}"),
        ("unrelated.rs", "pub fn missing() {}"),
    ] {
        std::fs::write(dir.join("src").join(file), text).unwrap();
    }
    let mut cg = CodeGraph::init_sync(&dir).unwrap();
    let result = cg.index_all(IndexOptions::default());
    assert!(result.success, "{:?}", result.errors);
    let file = cg
        .get_nodes_by_kind(NodeKind::File)
        .into_iter()
        .find(|n| n.file_path == "src/lib.rs")
        .unwrap();
    let imports = cg
        .get_outgoing_edges(&file.id)
        .into_iter()
        .filter(|e| e.kind == EdgeKind::Imports)
        .map(|e| e.target)
        .collect::<Vec<_>>();
    let functions = cg.get_nodes_by_kind(NodeKind::Function);
    for name in ["unique", "shared", "missing"] {
        let found = functions
            .iter()
            .filter(|n| n.name == name)
            .any(|n| imports.contains(&n.id));
        assert_eq!(found, name == "unique", "{name}");
    }
    cg.close();
    std::fs::remove_dir_all(dir).unwrap();
}
