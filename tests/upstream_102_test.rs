use rustcodegraph::types::EdgeKind;
use rustcodegraph::{CodeGraph, IndexOptions};

#[test]
fn case_102_import_scopes_and_missing_dependencies() {
    let dir = std::env::temp_dir().join(format!("rcg-102-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (path, source) in [
        (
            "lib.sol",
            "library Tools { function work() internal {} } contract Base {}",
        ),
        (
            "other.sol",
            "library Tools { function work() internal {} } contract Base {} contract Missing {}",
        ),
        (
            "main.sol",
            r#"import {Tools as T, Base} from "./lib.sol";
contract C is Base { function run() public { T.work(); new Missing(); } }"#,
        ),
        (
            "namespace.sol",
            r#"import * as L from "./lib.sol"; contract D is L.Base { function go() public { L.Tools.work(); } }"#,
        ),
        ("bare.sol", r#"import "./lib.sol"; contract E is Base {}"#),
        (
            "missing.sol",
            r#"import {Missing} from "@external/pkg.sol"; contract F is Missing {}"#,
        ),
        ("unimported.sol", "contract G is Base {}"),
        (
            "absent.sol",
            r#"import {Base} from "./absent-lib.sol"; contract H is Base {}"#,
        ),
        (
            "comment.sol",
            r#"// import "./lib.sol";
contract J is Base {}"#,
        ),
    ] {
        std::fs::write(dir.join(path), source).unwrap();
    }
    let mut graph = CodeGraph::init_sync(&dir).unwrap();
    let result = graph.index_all(IndexOptions::default());
    assert!(result.success, "{:?}", result.errors);
    let lib = graph.get_nodes_in_file("lib.sol");
    for (file, owner, target, kind) in [
        ("main.sol", "C", "Base", EdgeKind::Extends),
        ("main.sol", "run", "work", EdgeKind::Calls),
        ("namespace.sol", "D", "Base", EdgeKind::Extends),
        ("namespace.sol", "go", "work", EdgeKind::Calls),
        ("bare.sol", "E", "Base", EdgeKind::Extends),
    ] {
        let node = graph
            .get_nodes_in_file(file)
            .into_iter()
            .find(|n| n.name == owner)
            .unwrap();
        let target = lib.iter().find(|n| n.name == target).unwrap();
        assert!(
            graph
                .get_outgoing_edges(&node.id)
                .iter()
                .any(|e| e.target == target.id && e.kind == kind),
            "missing {file}: {owner} -> {}",
            target.name
        );
    }
    for file in ["main.sol", "namespace.sol", "bare.sol"] {
        let import = graph
            .get_nodes_in_file(file)
            .into_iter()
            .find(|n| n.kind == rustcodegraph::types::NodeKind::Import)
            .unwrap();
        assert!(
            graph
                .get_outgoing_edges(&import.id)
                .iter()
                .any(|e| e.kind == EdgeKind::Imports && e.target == "file:lib.sol")
        );
    }
    for (file, owner) in [
        ("missing.sol", "F"),
        ("unimported.sol", "G"),
        ("absent.sol", "H"),
        ("comment.sol", "J"),
        ("main.sol", "run"),
    ] {
        let node = graph
            .get_nodes_in_file(file)
            .into_iter()
            .find(|n| n.name == owner)
            .unwrap();
        assert!(
            !graph
                .get_outgoing_edges(&node.id)
                .iter()
                .any(|e| e.kind == EdgeKind::Instantiates || (e.kind == EdgeKind::Extends)),
            "unexpected {file}: {:?}",
            graph.get_outgoing_edges(&node.id)
        );
    }
    graph.close();
    std::fs::remove_dir_all(dir).unwrap();
}
