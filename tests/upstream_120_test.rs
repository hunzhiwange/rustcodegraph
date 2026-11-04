use rustcodegraph::{CodeGraph, types::EdgeKind};

#[test]
fn case_120_module_import_arity_and_missing_targets() {
    let dir = std::env::temp_dir().join(format!("rcg-120-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (file, source) in [
        (
            "main.erl",
            "-module(main).\n-import(worker, [go/1]).\n-include(\"include/defs.hrl\").\nrun() -> go(ok), worker:go(ok, ok), local(), absent:go(ok), missing().\nlocal() -> ok.",
        ),
        (
            "worker.erl",
            "-module(worker).\n-export([go/1, go/2]).\ngo(X) -> X.\ngo(X,Y) -> {X,Y}.",
        ),
        (
            "other.erl",
            "-module(other).\ngo(X) -> X.\nmissing() -> ok.\nlocal() -> ok.",
        ),
        ("include/defs.hrl", "-define(FLAG, true)."),
    ] {
        let path = dir.join(file);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, source).unwrap();
    }
    let mut g = CodeGraph::init_sync(&dir).unwrap();
    let result = g.index_all(Default::default());
    assert!(result.success, "{:?}", result.errors);
    let nodes = g.get_nodes_in_file("main.erl");
    let run = nodes.iter().find(|n| n.name == "run").unwrap();
    let edges = g.get_outgoing_edges(&run.id);
    for target in ["worker::go/1", "worker::go/2", "main::local/0"] {
        let file = if target.starts_with("worker") {
            "worker.erl"
        } else {
            "main.erl"
        };
        let n = g
            .get_nodes_in_file(file)
            .into_iter()
            .find(|n| n.qualified_name == target)
            .unwrap();
        assert!(
            edges
                .iter()
                .any(|e| e.kind == EdgeKind::Calls && e.target == n.id),
            "missing {target}"
        );
    }
    assert_eq!(
        edges.iter().filter(|e| e.kind == EdgeKind::Calls).count(),
        3,
        "{edges:?}"
    );
    let module = nodes.iter().find(|n| n.name == "main").unwrap();
    let imports = g.get_outgoing_edges(&module.id);
    assert!(
        imports
            .iter()
            .any(|e| e.kind == EdgeKind::Imports && e.target == "file:include/defs.hrl")
    );
    assert!(imports.iter().any(|e| {
        e.kind == EdgeKind::Imports
            && g.get_nodes_in_file("worker.erl")
                .iter()
                .any(|n| n.id == e.target)
    }));
    g.close();
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn case_120_absent_imports_and_wrong_arity_stay_unresolved() {
    let dir = std::env::temp_dir().join(format!("rcg-120-negative-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (file, source) in [
        (
            "main.erl",
            "-module(main).\n-import(absent, [go/1]).\n-include(\"missing.hrl\").\nrun() -> go(ok), absent:go(ok), unique:go(), not_imported(ok).",
        ),
        ("a.erl", "-module(first).\ngo(X) -> X."),
        ("b.erl", "-module(second).\ngo(Y) -> {Y}."),
        (
            "unique.erl",
            "-module(unique).\ngo(X) -> X.\nnot_imported(X) -> X.",
        ),
        ("missing.erl", "-module(missing)."),
    ] {
        std::fs::write(dir.join(file), source).unwrap();
    }
    let mut g = CodeGraph::init_sync(&dir).unwrap();
    assert!(g.index_all(Default::default()).success);
    for n in g.get_nodes_in_file("main.erl") {
        assert!(
            g.get_outgoing_edges(&n.id)
                .iter()
                .all(|e| !matches!(e.kind, EdgeKind::Calls | EdgeKind::Imports)),
            "unexpected binding for {}: {:?}",
            n.name,
            g.get_outgoing_edges(&n.id)
        );
    }
    g.close();
    std::fs::remove_dir_all(dir).unwrap();
}
