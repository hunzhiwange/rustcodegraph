use rustcodegraph::extraction::tree_sitter::TreeSitterExtractor;
use rustcodegraph::{
    CodeGraph,
    types::{EdgeKind, NodeKind},
};

const SOURCE: &str = "@Component\nexport struct Home {\n  build() { Text('你好') }\n  @Builder\n  card() { Text('card') }\n}\n@Builder\nexport function Header() { Text('header') }\nstruct Plain { build() {} }\n";

#[test]
fn case_190_declarations_and_contains() {
    let result = TreeSitterExtractor::new("Home.ets", SOURCE, None).extract();
    assert!(result.errors.is_empty());
    for (name, kind, qualified, start, end, exported) in [
        ("Home", NodeKind::Struct, "Home.ets::Home", 2, 6, true),
        (
            "build",
            NodeKind::Method,
            "Home.ets::Home::build",
            3,
            3,
            true,
        ),
        ("card", NodeKind::Method, "Home.ets::Home::card", 5, 5, true),
        ("Header", NodeKind::Function, "Home.ets::Header", 8, 8, true),
        ("Plain", NodeKind::Struct, "Home.ets::Plain", 9, 9, false),
    ] {
        let node = result
            .nodes
            .iter()
            .find(|n| n.qualified_name == qualified)
            .expect(qualified);
        assert_eq!(node.name, name);
        assert_eq!(node.kind, kind);
        assert_eq!((node.start_line, node.end_line), (start, end));
        assert_eq!(node.is_exported, Some(exported));
    }
    let home = result.nodes.iter().find(|n| n.name == "Home").unwrap();
    for qualified in ["Home.ets::Home::build", "Home.ets::Home::card"] {
        let member = result
            .nodes
            .iter()
            .find(|n| n.qualified_name == qualified)
            .unwrap();
        assert!(
            result.edges.iter().any(|e| e.kind == EdgeKind::Contains
                && e.source == home.id
                && e.target == member.id)
        );
    }
}

#[test]
fn case_190_no_local_or_decorator_phantoms() {
    let result = TreeSitterExtractor::new("Plain.ets", "struct Plain { build() { let local = 1; } }\n// @Component struct Fake {}\nconst text = '@Builder function Ghost() {}';", None).extract();
    assert!(
        result
            .nodes
            .iter()
            .any(|n| n.qualified_name == "Plain.ets::Plain::build")
    );
    for name in ["local", "Fake", "Ghost", "Component", "Builder"] {
        assert!(!result.nodes.iter().any(|n| n.name == name), "{name}");
    }
}

#[test]
fn case_190_repeat_index_is_stable() {
    let dir = std::env::temp_dir().join(format!("rcg-190-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("Home.ets"), SOURCE).unwrap();
    let mut graph = CodeGraph::init_sync(&dir).unwrap();
    assert!(graph.index_all(Default::default()).success);
    let first = graph.get_nodes_in_file("Home.ets");
    assert!(
        first
            .iter()
            .any(|n| n.qualified_name == "Home.ets::Home::card")
    );
    assert!(graph.index_all(Default::default()).success);
    let second = graph.get_nodes_in_file("Home.ets");
    assert_eq!(first.len(), second.len());
    for node in first {
        assert!(second.iter().any(|n| n.id == node.id));
    }
    graph.close();
    std::fs::remove_dir_all(dir).unwrap();
}
