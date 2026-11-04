use rustcodegraph::types::EdgeKind;
use rustcodegraph::{CodeGraph, IndexOptions};
#[test]
fn case_103_inherited_modifier_flow_and_scope_boundaries() {
    let dir = std::env::temp_dir().join(format!("rcg-103-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (file, source) in [
        (
            "middle.sol",
            "import './base.sol'; contract Middle is Base {}",
        ),
        (
            "private.sol",
            "contract Hidden { function secret() private {} }\ncontract Child is Hidden { function run() public { secret(); } }",
        ),
        (
            "base.sol",
            "contract Base { modifier guarded() { check(); _; } function check() internal {} }",
        ),
        (
            "sibling.sol",
            "import './base.sol'; contract Sibling is Base { modifier guarded() { wrong(); _; } function wrong() internal {} }",
        ),
        (
            "main.sol",
            "import './middle.sol'; contract Main is Middle { function run() public guarded {} }",
        ),
        (
            "unrelated.sol",
            "contract Unrelated { function run() public guarded {} }",
        ),
        (
            "ambiguous.sol",
            "contract A { modifier guarded() { _; } }\ncontract B { modifier guarded() { _; } }\ncontract C is A, B { function run() public guarded {} }",
        ),
    ] {
        std::fs::write(dir.join(file), source).unwrap();
    }
    let mut graph = CodeGraph::init_sync(&dir).unwrap();
    let result = graph.index_all(IndexOptions::default());
    assert!(result.success, "{:?}", result.errors);

    let node = |graph: &mut CodeGraph, file: &str, name: &str| {
        graph
            .get_nodes_in_file(file)
            .into_iter()
            .find(|n| n.name == name)
            .unwrap()
    };
    let run = node(&mut graph, "main.sol", "run");
    let guard = node(&mut graph, "base.sol", "guarded");
    let check = node(&mut graph, "base.sol", "check");
    for (from, to) in [(&run, &guard), (&guard, &check)] {
        assert!(
            graph
                .get_outgoing_edges(&from.id)
                .iter()
                .any(|e| e.kind == EdgeKind::Calls && e.target == to.id),
            "missing {} -> {}",
            from.qualified_name,
            to.qualified_name
        );
    }
    for file in ["unrelated.sol", "ambiguous.sol", "private.sol"] {
        let run = node(&mut graph, file, "run");
        assert!(
            !graph
                .get_outgoing_edges(&run.id)
                .iter()
                .any(|e| e.kind == EdgeKind::Calls),
            "unexpected binding in {file}: {:?}",
            graph.get_outgoing_edges(&run.id)
        );
    }
    graph.close();
    std::fs::remove_dir_all(dir).unwrap();
}
