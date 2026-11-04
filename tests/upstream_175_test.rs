use rustcodegraph::{CodeGraph, types::EdgeKind};

static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn check(source: &str, expected: &[(&str, EdgeKind)]) {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!(
        "rcg175-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    for (path, text) in [
        ("pages/Main.cfc", source),
        ("app/Service.cfc", "component { function save() {} }"),
        ("other/Service.cfc", "component { function save() {} }"),
        ("pages/Local.cfc", "component {}"),
        ("Local.cfc", "component {}"),
        ("pages/parts/view.cfm", "<cfoutput>hello</cfoutput>"),
        ("other/view.cfm", "<cfoutput>wrong</cfoutput>"),
    ] {
        let path = dir.as_path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    let mut graph = CodeGraph::init_sync(dir.as_path()).unwrap();
    assert!(graph.index_all(Default::default()).success);
    let mut actual = Vec::new();
    for node in graph.get_nodes_in_file("pages/Main.cfc") {
        for edge in graph.get_outgoing_edges(&node.id) {
            if matches!(
                edge.kind,
                EdgeKind::Imports | EdgeKind::Instantiates | EdgeKind::Calls | EdgeKind::Extends
            ) {
                actual.push((edge.target.clone(), edge.kind));
            }
        }
    }
    for (path, kind) in expected {
        let targets = graph.get_nodes_in_file(path);
        assert!(
            actual
                .iter()
                .any(|(id, k)| k == kind && targets.iter().any(|n| &n.id == id)),
            "missing {path} {kind:?}: {actual:?}"
        );
    }
    assert_eq!(actual.len(), expected.len(), "unexpected edges: {actual:?}");
    graph.close();
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn case_175_constructed_component_call() {
    check(
        "component { function run() { new app.Service().save(); createObject('component', 'other.Service').save(); } }",
        &[
            ("app/Service.cfc", EdgeKind::Instantiates),
            ("app/Service.cfc", EdgeKind::Calls),
            ("other/Service.cfc", EdgeKind::Instantiates),
            ("other/Service.cfc", EdgeKind::Calls),
        ],
    );
}
#[test]
fn case_175_missing_component_never_matches_method_name() {
    check(
        "component { function run() { new missing.Service().save(); createObject('component', 'missing.Service').save(); } }",
        &[],
    );
}
#[test]
fn case_175_dynamic_or_non_component_receiver_is_not_guessed() {
    check(
        "component { function run() { createObject('java', 'app.Service').save(); createObject('component', '#path#').save(); unknown.save(); } }",
        &[],
    );
}
#[test]
fn case_175_missing_method_is_not_guessed() {
    check(
        "component { function run() { new app.Service().destroy(); } }",
        &[("app/Service.cfc", EdgeKind::Instantiates)],
    );
}

#[test]
fn case_175_ambiguous_short_component_is_not_guessed() {
    check(
        "component { function run() { new Service().save(); } }",
        &[],
    );
}
