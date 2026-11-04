use rustcodegraph::{CodeGraph, types::EdgeKind};

static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn check(source: &str, expected: &[(&str, EdgeKind)]) {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!(
        "rcg174-{}-{}",
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
fn case_174_paths_and_component_disambiguation() {
    check(
        "component extends='APP.service' { function run() { new app.Service(); include './parts/../parts/view.cfm'; } }",
        &[
            ("app/Service.cfc", EdgeKind::Extends),
            ("app/Service.cfc", EdgeKind::Instantiates),
            ("pages/parts/view.cfm", EdgeKind::Imports),
        ],
    );
}

#[test]
fn case_174_missing_paths_do_not_use_basename() {
    check(
        "component { function run() { new vendor.Service(); include '../missing/view.cfm'; } }",
        &[],
    );
}

#[test]
fn case_174_bare_calls_do_not_leak_across_components() {
    check(
        "component { function run() { save(); unknown.save(); } }",
        &[],
    );
}

#[test]
fn case_174_local_component_and_method() {
    check(
        "component { function run() { new Local(); HeLpEr(); } function scoped() { this.helper(); } function scopedVariables() { variables.helper(); } function helper() {} }",
        &[
            ("pages/Local.cfc", EdgeKind::Instantiates),
            ("pages/Main.cfc", EdgeKind::Calls),
            ("pages/Main.cfc", EdgeKind::Calls),
            ("pages/Main.cfc", EdgeKind::Calls),
        ],
    );
}

#[test]
fn case_174_tag_include_and_created_component() {
    check(
        "<cfcomponent><cffunction name='run'><cfinclude template='parts/view.cfm'><cfscript>createObject('component','other.Service');</cfscript></cffunction></cfcomponent>",
        &[
            ("pages/parts/view.cfm", EdgeKind::Imports),
            ("other/Service.cfc", EdgeKind::Instantiates),
        ],
    );
}

#[test]
fn case_174_unmapped_absolute_and_ambiguous_short_names() {
    check(
        "component { function run() { new Service(); include '/other/view.cfm'; } }",
        &[],
    );
}
