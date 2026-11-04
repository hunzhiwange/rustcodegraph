use rustcodegraph::extraction::tree_sitter::TreeSitterExtractor;
use rustcodegraph::types::{NodeKind, Visibility};

const SOURCE: &str = "Namespace Company.Product\nPublic Class Widget\nPublic Property Title As String\nPrivate Event Changed()\nEnd Class\nPublic Module Tools\nPublic Property Count As Integer\nPublic Event Ready()\nEnd Module\nEnd Namespace\n";

#[test]
fn case_136_declarations_and_contains() {
    let result = TreeSitterExtractor::new("Sample.vb", SOURCE, None).extract();
    assert!(result.errors.is_empty());
    for (name, kind, start, end, parent) in [
        ("Company.Product", NodeKind::Namespace, 1, 11, None),
        ("Widget", NodeKind::Class, 2, 6, Some("Company.Product")),
        ("Title", NodeKind::Property, 3, 4, Some("Widget")),
        ("Changed", NodeKind::Field, 4, 5, Some("Widget")),
        ("Tools", NodeKind::Class, 6, 10, Some("Company.Product")),
        ("Count", NodeKind::Property, 7, 8, Some("Tools")),
        ("Ready", NodeKind::Field, 8, 9, Some("Tools")),
    ] {
        let node = result.nodes.iter().find(|n| n.name == name).expect(name);
        assert_eq!(node.kind, kind);
        assert_eq!((node.start_line, node.end_line), (start, end), "{name}");
        assert!(node.qualified_name.ends_with(name));
        if let Some(parent) = parent {
            let owner = result.nodes.iter().find(|n| n.name == parent).unwrap();
            assert_eq!(
                node.qualified_name,
                format!("{}::{name}", owner.qualified_name)
            );
            assert!(result.edges.iter().any(|e| e.source == owner.id
                && e.target == node.id
                && e.kind == rustcodegraph::types::EdgeKind::Contains));
        }
        assert_eq!(node.is_exported, Some(name != "Changed"));
    }
    let private = result.nodes.iter().find(|n| n.name == "Changed").unwrap();
    assert_eq!(private.visibility, Some(Visibility::Private));
}

#[test]
fn case_136_nested_scopes_and_no_local_symbols() {
    let source = "Namespace Outer\nNamespace Inner\npUbLiC Class Example\npRoTeCtEd FrIeNd Property Hidden As String\nPrivate Protected Event Secret()\nPublic Property Value As String\nGet\nDim local = \"Class Fake\"\nReturn local\nEnd Get\nEnd Property\nEnd Class\nEnd Namespace\nEnd Namespace";
    let result = TreeSitterExtractor::new("Nested.vb", source, None).extract();
    for name in ["Outer", "Inner", "Example", "Hidden", "Secret", "Value"] {
        assert_eq!(
            result.nodes.iter().filter(|n| n.name == name).count(),
            1,
            "{name}"
        );
    }
    assert!(
        !result
            .nodes
            .iter()
            .any(|n| ["local", "Fake", "Get"].contains(&n.name.as_str()))
    );
    for name in ["Hidden", "Secret"] {
        assert_eq!(
            result
                .nodes
                .iter()
                .find(|n| n.name == name)
                .unwrap()
                .is_exported,
            Some(false)
        );
    }
}

#[test]
fn case_136_sqlite_reindex_and_empty_comments() {
    let result =
        TreeSitterExtractor::new("Empty.vb", "' Class Fake\n' Event Ghost()\n", None).extract();
    assert_eq!(result.nodes.len(), 1);
    let dir = std::env::temp_dir().join(format!("rcg-136-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("Sample.vb"), SOURCE).unwrap();
    let mut graph = rustcodegraph::CodeGraph::init_sync(&dir).unwrap();
    assert!(graph.index_all(Default::default()).success);
    let first = graph.get_nodes_in_file("Sample.vb");
    assert_eq!(first.len(), 8);
    assert!(graph.index_all(Default::default()).success);
    let second = graph.get_nodes_in_file("Sample.vb");
    let mut a = first.iter().map(|n| n.id.clone()).collect::<Vec<_>>();
    let mut b = second.iter().map(|n| n.id.clone()).collect::<Vec<_>>();
    a.sort();
    b.sort();
    assert_eq!(a, b);
    graph.close();
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn case_136_friend_property_is_not_exported() {
    let source = "Public Class Publisher\nFriend Property InternalValue As Integer\nEnd Class\n";
    let result = TreeSitterExtractor::new("Events.vb", source, None).extract();
    let property = result
        .nodes
        .iter()
        .find(|n| n.name == "InternalValue")
        .unwrap();
    assert_eq!(property.visibility, Some(Visibility::Internal));
    assert_eq!(property.is_exported, Some(false));
}
