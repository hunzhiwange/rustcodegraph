use rustcodegraph::{
    CodeGraph,
    types::{EdgeKind, EdgeProvenance},
};

fn check(label: &str, ambiguous: bool, private: bool) {
    let dir = std::env::temp_dir().join(format!("rcg-121-{label}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut files = vec![
        ("contract.erl", "-module(contract).\n-callback decode(binary(), list()) -> term().\n".to_string()),
        ("runner.erl", "-module(runner).\nrun(M) -> M:decode(<<1,2,3>>, []).\nwrong(M) -> M:decode(ok).\nmacro() -> ?MODULE:decode(ok, []).\n".to_string()),
        ("impl.erl", format!("-module(impl).\n-behaviour(contract).\n{}\ndecode(X, _) -> sink(X).\nsink(X) -> X.\n", if private { "" } else { "-export([decode/2])." })),
        ("stranger.erl", "-module(stranger).\n-export([decode/2]).\ndecode(X, _) -> X.\n".to_string()),
        ("arity.erl", "-module(arity).\n-behaviour(contract).\n-export([decode/1]).\ndecode(X) -> X.\n".to_string()),
    ];
    if ambiguous {
        files.push((
            "other.erl",
            "-module(other).\n-callback decode(binary(), list()) -> term().".into(),
        ));
    }
    for (file, source) in files {
        std::fs::write(dir.join(file), source).unwrap();
    }
    let mut g = CodeGraph::init_sync(&dir).unwrap();
    let result = g.index_all(Default::default());
    assert!(result.success, "{:?}", result.errors);
    let nodes = g.get_nodes_in_file("runner.erl");
    assert!(nodes.iter().any(|n| n.name == "run"));
    for node in &nodes {
        let edges: Vec<_> = g
            .get_outgoing_edges(&node.id)
            .into_iter()
            .filter(|e| e.provenance == Some(EdgeProvenance::Heuristic))
            .collect();
        if node.name == "run" && !ambiguous && !private {
            assert_eq!(edges.len(), 1, "dispatch bridge missing: {edges:?}");
            let target = g
                .get_nodes_in_file("impl.erl")
                .into_iter()
                .find(|n| n.name == "decode")
                .unwrap();
            assert_eq!(edges[0].target, target.id);
            assert_eq!(edges[0].kind, EdgeKind::Calls);
            let meta = edges[0].metadata.as_ref().unwrap();
            assert_eq!(meta["synthesizedBy"], "erlang-behaviour");
            assert_eq!(meta["via"], "contract:decode/2");
            assert_eq!(meta["registeredAt"], "runner.erl:2");
            let sink = g
                .get_nodes_in_file("impl.erl")
                .into_iter()
                .find(|n| n.name == "sink")
                .unwrap();
            assert!(
                g.get_outgoing_edges(&target.id)
                    .iter()
                    .any(|e| e.kind == EdgeKind::Calls && e.target == sink.id)
            );
        } else {
            assert!(edges.is_empty(), "unexpected bridge: {edges:?}");
        }
    }
    g.close();
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn case_121_exported_callback_connects_to_sink() {
    check("positive", false, false);
}
#[test]
fn case_121_ambiguous_contract_is_silent() {
    check("ambiguous", true, false);
}
#[test]
fn case_121_private_callback_is_silent() {
    check("private", false, true);
}
