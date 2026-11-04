use rustcodegraph::{CodeGraph, types::EdgeProvenance};

fn check(label: &str, methods: &str, writers: &[&str]) {
    check_component(label, methods, writers, "@Component");
}

fn check_component(label: &str, methods: &str, writers: &[&str], decorator: &str) {
    let dir = std::env::temp_dir().join(format!("rcg-193-{}-{label}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("Child.ets"),
        "@Component\nexport struct Child { build() { Text('child') } }",
    )
    .unwrap();
    std::fs::write(dir.join("Home.ets"), format!("import {{ Child }} from './Child';\n{decorator}\nstruct Home {{\n @State count: number = 0;\n @State items: string[] = [];\n plain: number = 0;\n {methods}\n build() {{ Child() }}\n}}\n")).unwrap();
    let mut graph = CodeGraph::init_sync(&dir).unwrap();
    assert!(graph.index_all(Default::default()).success);
    let nodes = graph.get_nodes_in_file("Home.ets");
    let build = nodes.iter().find(|n| n.name == "build").unwrap();
    let child = graph
        .get_nodes_in_file("Child.ets")
        .into_iter()
        .find(|n| n.name == "Child")
        .unwrap();
    assert!(
        graph
            .get_callees(&build.id, 1)
            .iter()
            .any(|n| n.node.id == child.id),
        "static child component edge"
    );
    for method in nodes
        .iter()
        .filter(|n| n.kind == rustcodegraph::types::NodeKind::Method && n.name != "build")
    {
        let edges: Vec<_> = graph
            .get_outgoing_edges(&method.id)
            .into_iter()
            .filter(|e| e.target == build.id)
            .collect();
        assert_eq!(
            edges.len(),
            usize::from(writers.contains(&method.name.as_str())),
            "{}: {edges:?}",
            method.name
        );
        for edge in edges {
            assert_eq!(edge.provenance, Some(EdgeProvenance::Heuristic));
            let meta = edge.metadata.unwrap();
            assert_eq!(meta["synthesizedBy"], "arkui-state");
            assert!(
                meta["registeredAt"]
                    .as_str()
                    .unwrap()
                    .starts_with("Home.ets:")
            );
            assert!(
                graph
                    .get_callees(&method.id, 2)
                    .iter()
                    .any(|n| n.node.id == child.id),
                "state -> build -> child"
            );
        }
    }
    let before: Vec<_> = ["Home.ets", "Child.ets"]
        .into_iter()
        .flat_map(|f| graph.get_nodes_in_file(f))
        .map(|n| n.id)
        .collect();
    let edge_count: usize = before
        .iter()
        .map(|id| graph.get_outgoing_edges(id).len())
        .sum();
    assert!(graph.index_all(Default::default()).success);
    let after: Vec<_> = ["Home.ets", "Child.ets"]
        .into_iter()
        .flat_map(|f| graph.get_nodes_in_file(f))
        .map(|n| n.id)
        .collect();
    assert_eq!(before, after, "synthesis must not grow nodes on rebuild");
    assert_eq!(
        edge_count,
        after
            .iter()
            .map(|id| graph.get_outgoing_edges(id).len())
            .sum::<usize>()
    );
    graph.close();
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn case_193_state_to_build_to_imported_child() {
    check(
        "writers",
        "increment() { this.count++; }\n reset() { this.count = 0; }\n append() { this.items.push('x'); }",
        &["increment", "reset", "append"],
    );
}
#[test]
fn case_193_reads_plain_fields_and_comparisons_do_not_render() {
    check(
        "reads",
        "read() { return this.count; }\n compare() { return this.count === 1; }\n plainWrite() { this.plain++; }",
        &[],
    );
}
#[test]
fn case_193_comments_strings_and_other_receivers_do_not_render() {
    check(
        "decoys",
        "comment() { /* this.count++; */ }\n text() { return 'this.count = 1'; }\n other(other: Home) { other.count++; }",
        &[],
    );
}

#[test]
fn case_193_nested_closures_and_plain_structs_do_not_render() {
    check(
        "closure",
        "deferred() { return () => { this.count++; }; }\n objectMethod() { return { run() { this.count++; } }; }",
        &[],
    );
    check_component("plain-struct", "write() { this.count++; }", &[], "");
}
#[test]
fn case_193_compound_and_prefix_writes_render() {
    check(
        "compound",
        "add() { this.count += 1; }\n prefix() { ++this.count; }",
        &["add", "prefix"],
    );
}
