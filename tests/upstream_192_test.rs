use rustcodegraph::CodeGraph;

fn check(label: &str, source: &str, expected: &[(&str, &str)]) {
    let dir = std::env::temp_dir().join(format!("rcg-192-{}-{label}", std::process::id()));
    std::fs::create_dir_all(dir.join("lib")).unwrap();
    for (file, text) in [
        ("Home.ets", source),
        ("lib/one.ets", "export function work() {}"),
        ("lib/two.ets", "export function work() {}"),
        ("lib/index.ets", "export function helper() {}"),
        (
            "decoy.ets",
            "export function missing() {} export function remote() {}",
        ),
    ] {
        std::fs::write(dir.join(file), text).unwrap();
    }
    let mut graph = CodeGraph::init_sync(&dir).unwrap();
    assert!(graph.index_all(Default::default()).success);
    let nodes = graph.get_nodes_in_file("Home.ets");
    let caller = nodes.iter().find(|n| n.name == "build").unwrap();
    let mut actual: Vec<_> = graph
        .get_callees(&caller.id, 1)
        .into_iter()
        .map(|e| (e.node.name, e.node.file_path))
        .collect();
    actual.sort();
    actual.dedup();
    let mut expected: Vec<_> = expected
        .iter()
        .map(|(n, p)| (n.to_string(), p.to_string()))
        .collect();
    expected.sort();
    graph.close();
    std::fs::remove_dir_all(dir).unwrap();
    assert_eq!(actual, expected);
}

#[test]
fn case_192_relative_alias_disambiguation() {
    check(
        "alias",
        "import { work as first } from './lib/one'; import { work as second } from './lib/two.ets'; struct Home { build() { first(); second(); } }",
        &[("work", "lib/one.ets"), ("work", "lib/two.ets")],
    );
}
#[test]
fn case_192_directory_namespace() {
    check(
        "namespace",
        "import * as tools from './lib'; struct Home { build() { tools.helper(); } }",
        &[("helper", "lib/index.ets")],
    );
}
#[test]
fn case_192_missing_and_external_stay_unresolved() {
    check(
        "missing",
        "import { missing } from './absent'; import { remote } from '@ohos.remote'; struct Home { build() { missing(); remote(); } }",
        &[],
    );
}

#[test]
fn case_192_unimported_symbol_stays_unresolved() {
    check("unimported", "struct Home { build() { missing(); } }", &[]);
}
#[test]
fn case_192_local_calls_are_preserved() {
    check(
        "local",
        "function local() {}
 struct Home { build() { local(); this.tap(); } tap() {} }",
        &[("local", "Home.ets"), ("tap", "Home.ets")],
    );
}

#[test]
fn case_192_native_import_nodes_and_file_edges() {
    use rustcodegraph::types::{EdgeKind, NodeKind};
    let dir = std::env::temp_dir().join(format!("rcg-192-imports-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("Home.ets"),
        "import { work } from './other';\nstruct Home { build() { work(); } }",
    )
    .unwrap();
    std::fs::write(dir.join("other.ets"), "export function work() {}").unwrap();
    let mut graph = CodeGraph::init_sync(&dir).unwrap();
    assert!(graph.index_all(Default::default()).success);
    let imports: Vec<_> = graph
        .get_nodes_in_file("Home.ets")
        .into_iter()
        .filter(|n| n.kind == NodeKind::Import)
        .collect();
    assert!(!imports.is_empty(), "native import nodes must be persisted");
    let edges: Vec<_> = graph
        .get_nodes_in_file("Home.ets")
        .iter()
        .flat_map(|n| graph.get_outgoing_edges(&n.id))
        .collect();
    assert!(
        edges
            .iter()
            .any(|e| e.kind == EdgeKind::Imports && e.target == "file:other.ets"),
        "{edges:?}"
    );
    graph.close();
    std::fs::remove_dir_all(dir).unwrap();
}
