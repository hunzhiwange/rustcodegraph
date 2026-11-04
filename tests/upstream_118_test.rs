use rustcodegraph::extraction::index::extract_from_source;
use rustcodegraph::types::{EdgeKind, NodeKind};

#[test]
fn case_118_module_arity_clauses_exports() {
    let source = "-module('sample').\n-export([run/1]).\nrun(0) -> ok;\nrun(N) -> N.\nrun(A, B) -> {A, B}.\n";
    let r = extract_from_source("sample.erl", source, None, None);
    let module = r
        .nodes
        .iter()
        .find(|n| n.kind == NodeKind::Namespace)
        .unwrap();
    assert_eq!(module.name, "sample");
    let functions: Vec<_> = r
        .nodes
        .iter()
        .filter(|n| n.kind == NodeKind::Function)
        .collect();
    assert_eq!(functions.len(), 2);
    for (qualified, start, end, exported) in [
        ("sample::run/1", 3, 4, true),
        ("sample::run/2", 5, 5, false),
    ] {
        let n = functions
            .iter()
            .find(|n| n.qualified_name == qualified)
            .unwrap();
        assert_eq!(n.name, "run");
        assert_eq!((n.start_line, n.end_line), (start, end));
        assert_eq!(n.is_exported, Some(exported));
        assert!(
            r.edges
                .iter()
                .any(|e| e.kind == EdgeKind::Contains && e.source == module.id && e.target == n.id)
        );
    }
    assert_eq!(r.nodes.len(), 4);
}

#[test]
fn case_118_no_local_or_fake_declarations_and_no_export_leak() {
    for (source, exported) in [
        (
            "-module(m). -compile([export_all]). f() -> X = fun(Y) -> Y end, X(1).",
            true,
        ),
        (
            "-module(m). % -export([f/0]).\nf() -> \"fake(A) -> A.\".",
            false,
        ),
        ("f() -> ok.", false),
    ] {
        let r = extract_from_source("same.erl", source, None, None);
        let functions: Vec<_> = r
            .nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Function)
            .collect();
        assert_eq!(functions.len(), 1);
        assert_eq!(functions[0].is_exported, Some(exported));
        assert!(!r.nodes.iter().any(|n| n.kind == NodeKind::Variable));
    }
}

#[test]
fn case_118_sqlite_reindex() {
    let dir = std::env::temp_dir().join(format!("rustcodegraph-118-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("m.erl"),
        "-module(m). -export([f/0]). f() -> ok; f() -> error. f(X) -> X.",
    )
    .unwrap();
    let mut graph = rustcodegraph::CodeGraph::init_sync(&dir).unwrap();
    for _ in 0..2 {
        let result = graph.index_all(Default::default());
        assert!(result.success, "{:?}", result.errors);
        assert_eq!(graph.get_nodes_in_file("m.erl").len(), 4);
    }
    graph.close();
    std::fs::remove_dir_all(dir).unwrap();
}
