use rustcodegraph::extraction::index::extract_from_source;
use rustcodegraph::types::{EdgeKind, NodeKind};

const SOURCE: &str = "pragma solidity ^0.8.0;\ncontract Vault {\n struct Entry { uint amount; }\n event Changed(uint amount);\n modifier guarded() { uint local = 1; _; }\n}\ninterface IVault { event Changed(uint amount); }\nlibrary Helpers { struct Entry { uint value; } }\n";

#[test]
fn case_100_declarations_and_contains() {
    let result = extract_from_source("vault.sol", SOURCE, None, None);
    for (name, qualified, kind, start, end, parent) in [
        (
            "Vault",
            "vault.sol::Vault",
            NodeKind::Class,
            2,
            6,
            "vault.sol",
        ),
        (
            "Entry",
            "vault.sol::Vault::Entry",
            NodeKind::Struct,
            3,
            3,
            "Vault",
        ),
        (
            "Changed",
            "vault.sol::Vault::Changed",
            NodeKind::Field,
            4,
            4,
            "Vault",
        ),
        (
            "guarded",
            "vault.sol::Vault::guarded",
            NodeKind::Method,
            5,
            5,
            "Vault",
        ),
        (
            "IVault",
            "vault.sol::IVault",
            NodeKind::Interface,
            7,
            7,
            "vault.sol",
        ),
        (
            "Changed",
            "vault.sol::IVault::Changed",
            NodeKind::Field,
            7,
            7,
            "IVault",
        ),
        (
            "Helpers",
            "vault.sol::Helpers",
            NodeKind::Class,
            8,
            8,
            "vault.sol",
        ),
        (
            "Entry",
            "vault.sol::Helpers::Entry",
            NodeKind::Struct,
            8,
            8,
            "Helpers",
        ),
    ] {
        let node = result
            .nodes
            .iter()
            .find(|n| n.qualified_name == qualified)
            .unwrap_or_else(|| panic!("missing {qualified}"));
        assert_eq!(
            (&*node.name, node.kind, node.start_line, node.end_line),
            (name, kind, start, end)
        );
        assert_eq!(
            node.is_exported,
            Some(false),
            "match upstream default export semantics"
        );
        let owner = result
            .nodes
            .iter()
            .find(|n| {
                n.qualified_name == format!("vault.sol::{parent}")
                    || (parent == "vault.sol" && n.kind == NodeKind::File)
            })
            .unwrap();
        assert!(
            result.edges.iter().any(|e| e.kind == EdgeKind::Contains
                && e.source == owner.id
                && e.target == node.id)
        );
    }
    assert_eq!(result.nodes.len(), 9);
}

#[test]
fn case_100_no_comment_string_or_local_declarations() {
    let source = "// contract Fake {}\ncontract Real { string text = 'event Fake(uint x);'; function run() public { uint local = 1; } }";
    let result = extract_from_source("real.sol", source, None, None);
    assert_eq!(
        result
            .nodes
            .iter()
            .filter(|n| n.kind != NodeKind::File)
            .map(|n| n.name.as_str())
            .collect::<Vec<_>>(),
        vec!["Real", "run"]
    );
    let empty = extract_from_source("empty.sol", "// struct Fake {}", None, None);
    assert_eq!(empty.nodes.len(), 1);
}

#[test]
fn case_100_reindex_is_stable_in_sqlite() {
    let dir = std::env::temp_dir().join(format!("rustcodegraph-100-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("vault.sol"), SOURCE).unwrap();
    let mut graph = rustcodegraph::CodeGraph::init_sync(&dir).unwrap();
    for _ in 0..2 {
        let result = graph.index_all(rustcodegraph::IndexOptions::default());
        assert!(result.success, "{:?}", result.errors);
        assert_eq!(graph.get_nodes_in_file("vault.sol").len(), 9);
    }
    graph.close();
    std::fs::remove_dir_all(dir).unwrap();
}
