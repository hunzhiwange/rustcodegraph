use rustcodegraph::extraction::index::extract_from_source;
use rustcodegraph::types::ReferenceKind;

#[test]
fn case_101_inheritance_modifiers_constructors_and_members() {
    let source = "contract Vault is Base {\n modifier guarded() { check(); _; }\n constructor() Base(seed()) { init(); }\n function run() public guarded { token.transfer(amount()); new Child(); }\n}";
    let r = extract_from_source("vault.sol", source, None, None);
    for (owner, target, kind) in [
        ("Vault", "Base", ReferenceKind::Extends),
        ("guarded", "check", ReferenceKind::Calls),
        ("constructor", "Base", ReferenceKind::Calls),
        ("constructor", "seed", ReferenceKind::Calls),
        ("constructor", "init", ReferenceKind::Calls),
        ("run", "guarded", ReferenceKind::Calls),
        ("run", "token.transfer", ReferenceKind::Calls),
        ("run", "amount", ReferenceKind::Calls),
        ("run", "Child", ReferenceKind::Instantiates),
    ] {
        let node = r
            .nodes
            .iter()
            .find(|n| n.name == owner)
            .unwrap_or_else(|| panic!("missing {owner}"));
        assert!(
            r.unresolved_references
                .iter()
                .any(|v| v.from_node_id == node.id
                    && v.reference_name == target
                    && v.reference_kind == kind),
            "missing {owner} -> {target}: {:?}",
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
fn case_101_comments_strings_and_distinct_owners() {
    let source = "// fake()\ncontract C { modifier guard() { before(); _; } function a() public { string memory s = 'fake()'; one(); } function b() public { two(); } }";
    let r = extract_from_source("c.sol", source, None, None);
    assert_eq!(r.unresolved_references.len(), 3);
    for (owner, target) in [("guard", "before"), ("a", "one"), ("b", "two")] {
        let id = &r.nodes.iter().find(|n| n.name == owner).unwrap().id;
        assert!(
            r.unresolved_references
                .iter()
                .any(|v| &v.from_node_id == id && v.reference_name == target)
        );
    }
}

#[test]
fn case_101_multibyte_positions() {
    let source =
        "contract C { function run() public { string memory s = unicode\"你好\"; go(); } }";
    let r = extract_from_source("c.sol", source, None, None);
    let call = r
        .unresolved_references
        .iter()
        .find(|v| v.reference_name == "go")
        .unwrap();
    assert_eq!(call.line, 1);
    assert_eq!(call.column, source.find("go()").unwrap() as u64);
}

#[test]
fn case_101_sqlite_reindex_stability() {
    let dir = std::env::temp_dir().join(format!("rustcodegraph-101-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("c.sol"),
        "contract C { function run() public { ping(); } function ping() internal {} }",
    )
    .unwrap();
    let mut graph = rustcodegraph::CodeGraph::init_sync(&dir).unwrap();
    for _ in 0..2 {
        let result = graph.index_all(rustcodegraph::IndexOptions::default());
        assert!(result.success, "{:?}", result.errors);
        assert_eq!(graph.get_nodes_in_file("c.sol").len(), 4);
    }
    graph.close();
    std::fs::remove_dir_all(dir).unwrap();
}
