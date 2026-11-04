use rustcodegraph::extraction::index::extract_from_source;
use rustcodegraph::types::ReferenceKind;

#[test]
fn case_119_calls_directives_and_macro_ownership() {
    let s = "-module(m).\n-behaviour(gen_server).\n-include(\"x.hrl\").\n-include_lib(\"app/include/y.hrl\").\n-import(lists, [reverse/1]).\n-define(LOG(X), audit:log(X)).\nf(0) -> local(), lists:reverse([]), ?LOG(ok);\nf(X) -> fun local/0, fun lists:reverse/1, fun(Y) -> nested(Y) end.\n";
    let r = extract_from_source("m.erl", s, None, None);
    for (name, kind, owner) in [
        ("gen_server", ReferenceKind::Implements, "m"),
        ("x.hrl", ReferenceKind::Imports, "m"),
        ("app/include/y.hrl", ReferenceKind::Imports, "m"),
        ("lists", ReferenceKind::Imports, "m"),
        ("audit::log/1", ReferenceKind::Calls, "LOG"),
        ("local/0", ReferenceKind::Calls, "f"),
        ("local/0", ReferenceKind::References, "f"),
        ("lists::reverse/1", ReferenceKind::References, "f"),
        ("lists::reverse/1", ReferenceKind::Calls, "f"),
        ("LOG", ReferenceKind::Calls, "f"),
        ("nested/1", ReferenceKind::Calls, "f"),
    ] {
        assert!(
            r.unresolved_references
                .iter()
                .any(|x| x.reference_name == name
                    && x.reference_kind == kind
                    && r.nodes
                        .iter()
                        .any(|n| n.id == x.from_node_id && n.name == owner)),
            "missing {name}: {:?}",
            r.unresolved_references
        );
    }
    assert!(
        r.edges
            .iter()
            .all(|e| e.kind == rustcodegraph::types::EdgeKind::Contains)
    );
}

#[test]
fn case_119_no_fake_type_or_dynamic_calls() {
    let r = extract_from_source(
        "m.erl",
        "-module(m).\n-spec f(term()) -> pid().\n-callback cb(term()) -> ok.\n-type t() :: integer().\n% fake:call().\nf(X) -> \"fake:call()\", Mod:run(X), X(), ?MODULE:local(X), real:go(inner()).",
        None,
        None,
    );
    let names: Vec<_> = r
        .unresolved_references
        .iter()
        .filter(|x| x.reference_kind == ReferenceKind::Calls)
        .map(|x| x.reference_name.as_str())
        .collect();
    assert_eq!(names, ["local/1", "real::go/1", "inner/0"]);
}

#[test]
fn case_119_unicode_positions_and_sqlite() {
    let s = "-module(m).\nf() -> \"你好🌍\", other:run().\n";
    let r = extract_from_source("m.erl", s, None, None);
    let call = r
        .unresolved_references
        .iter()
        .find(|x| x.reference_name == "other::run/0")
        .unwrap();
    assert_eq!(call.line, 2);
    assert_eq!(
        call.column,
        s.lines().nth(1).unwrap().find("other").unwrap() as u64
    );
    let dir = std::env::temp_dir().join(format!("rcg-119-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("m.erl"), s).unwrap();
    let mut graph = rustcodegraph::CodeGraph::init_sync(&dir).unwrap();
    assert!(graph.index_all(Default::default()).success);
    assert!(
        graph
            .get_nodes_in_file("m.erl")
            .iter()
            .any(|n| n.name == "f")
    );
    graph.close();
    std::fs::remove_dir_all(dir).unwrap();
}
