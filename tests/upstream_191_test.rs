use rustcodegraph::extraction::tree_sitter::TreeSitterExtractor;
use rustcodegraph::types::ReferenceKind;

#[test]
fn case_191_components_attributes_and_handlers() {
    let source = "@Component struct Home {\n @State count: number = 0;\n update() { this.count++; refresh() }\n build() { Column() { Child({value: this.count}).fontSize(16).onClick(this.update) } }\n}";
    let result = TreeSitterExtractor::new("Home.ets", source, None).extract();
    assert!(result.errors.is_empty());
    let build = result.nodes.iter().find(|n| n.name == "build").unwrap();
    for name in ["Column", "Child", ".fontSize", ".onClick", "update"] {
        assert!(
            result
                .unresolved_references
                .iter()
                .any(|r| r.from_node_id == build.id
                    && r.reference_name == name
                    && r.reference_kind == ReferenceKind::Calls),
            "missing {name}: {:?}",
            result.unresolved_references
        );
    }
    let update = result.nodes.iter().find(|n| n.name == "update").unwrap();
    assert!(
        result
            .unresolved_references
            .iter()
            .any(|r| r.from_node_id == update.id && r.reference_name == "refresh")
    );
    assert!(
        !result
            .edges
            .iter()
            .any(|e| e.kind == rustcodegraph::types::EdgeKind::Calls)
    );
}

#[test]
fn case_191_no_comment_string_or_non_event_binding_calls() {
    let result = TreeSitterExtractor::new("Home.ets", "struct Home { build() { /* Fake() */ let text = 'Ghost()'; Text(text).width(this.size).onClick(other.handler) } }", None).extract();
    let names: Vec<_> = result
        .unresolved_references
        .iter()
        .filter(|r| r.reference_kind == ReferenceKind::Calls)
        .map(|r| r.reference_name.as_str())
        .collect();
    assert!(names.contains(&"Text"));
    for name in ["Fake", "Ghost", "size", "handler"] {
        assert!(!names.contains(&name), "{name}");
    }
}

#[test]
fn case_191_nested_owner_and_multibyte_positions() {
    let source = "struct Home { build() { function nested() { ping() } let label = '你好'; this.load(); } load() {} }";
    let result = TreeSitterExtractor::new("Home.ets", source, None).extract();
    for (owner, target, offset) in [
        ("nested", "ping", source.find("ping()").unwrap()),
        ("build", "load", source.find("this.load()").unwrap()),
    ] {
        let node = result.nodes.iter().find(|n| n.name == owner).unwrap();
        let reference = result
            .unresolved_references
            .iter()
            .find(|r| r.from_node_id == node.id && r.reference_name == target)
            .expect(target);
        assert_eq!((reference.line, reference.column), (1, offset as u64));
    }
}

#[test]
fn case_191_detached_chain_and_sqlite_stability() {
    let source = "struct Home { build() { Column() { Row() { Text('你好') }\n.onClick(this.tap)\n.width(20)\n } } tap() { work() } }";
    let result = TreeSitterExtractor::new("Home.ets", source, None).extract();
    let build = result.nodes.iter().find(|n| n.name == "build").unwrap();
    for name in ["Row", "Text", ".onClick", "tap", ".width"] {
        assert!(
            result
                .unresolved_references
                .iter()
                .any(|r| r.from_node_id == build.id && r.reference_name == name),
            "missing {name}: {:?}",
            result.unresolved_references
        );
    }
    let dir = std::env::temp_dir().join(format!("rcg-191-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("Home.ets"), source).unwrap();
    let mut graph = rustcodegraph::CodeGraph::init_sync(&dir).unwrap();
    assert!(graph.index_all(Default::default()).success);
    let first = graph.get_nodes_in_file("Home.ets");
    assert!(graph.index_all(Default::default()).success);
    let second = graph.get_nodes_in_file("Home.ets");
    assert_eq!(first.len(), second.len());
    assert!(first.iter().all(|n| second.iter().any(|m| m.id == n.id)));
    graph.close();
    std::fs::remove_dir_all(dir).unwrap();
}
