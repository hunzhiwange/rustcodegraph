use rustcodegraph::types::{EdgeKind, NodeKind};
use rustcodegraph::{CodeGraph, IndexOptions};
#[test]
fn case_022_visible_source_sets() {
    let dir = std::env::temp_dir().join(format!("rcg22-{}", std::process::id()));
    for (set, text) in [
        ("commonMain", "package p\nexpect class Thing"),
        ("androidMain", "package p\nactual class Thing"),
        ("iosMain", "package p\nactual class Thing"),
        ("androidUnitTest", "package q\nimport p.Thing\nclass Use"),
        ("commonTest", "package q\nimport p.Thing\nclass Use"),
        ("desktopMain", "package q\nimport p.Thing\nclass Use"),
    ] {
        let p = dir.join(format!("src/{set}/kotlin/Test.kt"));
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }
    let unknown = dir.join("sample/Use.kt");
    std::fs::create_dir_all(unknown.parent().unwrap()).unwrap();
    std::fs::write(unknown, "package q\nimport p.Thing\nclass Sample").unwrap();
    let mut cg = CodeGraph::init_sync(&dir).unwrap();
    let r = cg.index_all(IndexOptions::default());
    assert!(r.success, "{:?}", r.errors);
    let classes = cg.get_nodes_by_kind(NodeKind::Class);
    for (path, want) in [
        ("src/androidUnitTest/kotlin/Test.kt", "androidMain"),
        ("src/commonTest/kotlin/Test.kt", "commonMain"),
        ("src/desktopMain/kotlin/Test.kt", "commonMain"),
        ("sample/Use.kt", "commonMain"),
    ] {
        let mut targets = Vec::new();
        for kind in [NodeKind::File, NodeKind::Import, NodeKind::Namespace] {
            for n in cg
                .get_nodes_by_kind(kind)
                .into_iter()
                .filter(|n| n.file_path == path)
            {
                targets.extend(
                    cg.get_outgoing_edges(&n.id)
                        .into_iter()
                        .filter(|e| e.kind == EdgeKind::Imports)
                        .map(|e| e.target),
                );
            }
        }
        let matched = classes
            .iter()
            .filter(|n| n.name == "Thing" && targets.contains(&n.id))
            .collect::<Vec<_>>();
        assert_eq!(matched.len(), 1, "{path}, classes={classes:?}");
        assert!(
            matched[0].file_path.contains(want),
            "{path}: {}",
            matched[0].file_path
        );
    }
    cg.close();
    std::fs::remove_dir_all(dir).unwrap();
}
