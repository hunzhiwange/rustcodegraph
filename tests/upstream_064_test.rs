use rustcodegraph::extraction::index::extract_from_source;
use rustcodegraph::types::{EdgeKind, NodeKind};

#[test]
fn case_064_declarations_and_contains() {
    let source = "let\n  value = 42;\n  add = x: y: x + y;\n  factory = { name, ... }@args: { result = name; };\nin {\n  public = add;\n  run = x: x;\n  nested = { hidden = 1; };\n  inherit value;\n}\n";
    let result = extract_from_source("default.nix", source, None, None);
    for (name, kind, exported, start, end) in [
        ("value", NodeKind::Variable, false, 2, 2),
        ("add", NodeKind::Function, false, 3, 3),
        ("factory", NodeKind::Function, false, 4, 4),
        ("result", NodeKind::Variable, false, 4, 4),
        ("public", NodeKind::Variable, true, 6, 6),
        ("run", NodeKind::Function, true, 7, 7),
        ("nested", NodeKind::Variable, true, 8, 8),
        ("hidden", NodeKind::Variable, false, 8, 8),
    ] {
        let n = result
            .nodes
            .iter()
            .find(|n| n.name == name)
            .unwrap_or_else(|| panic!("missing {name}"));
        assert_eq!(
            (n.kind, n.is_exported, n.start_line, n.end_line),
            (kind, Some(exported), start, end)
        );
        let owner = if name == "result" {
            "factory"
        } else {
            "default.nix"
        };
        assert_eq!(
            n.qualified_name,
            format!(
                "default.nix::{}{}",
                if owner == "factory" { "factory::" } else { "" },
                name
            )
        );
        let parent = result.nodes.iter().find(|n| n.name == owner).unwrap();
        assert!(
            result
                .edges
                .iter()
                .any(|e| e.kind == EdgeKind::Contains && e.source == parent.id && e.target == n.id)
        );
    }
    assert_eq!(
        result
            .nodes
            .iter()
            .find(|n| n.name == "add")
            .unwrap()
            .signature
            .as_deref(),
        Some("x : y")
    );
    assert_eq!(
        result
            .nodes
            .iter()
            .find(|n| n.name == "factory")
            .unwrap()
            .signature
            .as_deref(),
        Some("{ name, ... }@args")
    );
    assert!(
        result
            .nodes
            .iter()
            .any(|n| n.name == "value" && n.start_line == 9 && n.is_exported == Some(true))
    );
}

#[test]
fn case_064_anonymous_and_local_boundaries() {
    let result = extract_from_source(
        "default.nix",
        "{ config, ... }: let helper = x: let scratch = x; in scratch; in { answer = helper 1; }",
        None,
        None,
    );
    assert!(
        !result
            .nodes
            .iter()
            .any(|n| ["config", "scratch", "<anonymous>", "x"].contains(&n.name.as_str()))
    );
    assert_eq!(
        result
            .nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Function)
            .count(),
        1
    );
    assert_eq!(
        result
            .nodes
            .iter()
            .find(|n| n.name == "helper")
            .unwrap()
            .is_exported,
        Some(false)
    );
    assert_eq!(
        result
            .nodes
            .iter()
            .find(|n| n.name == "answer")
            .unwrap()
            .is_exported,
        Some(true)
    );
}

#[test]
fn case_064_dynamic_attributes_and_empty_input() {
    for source in ["", "{}", "{ ${key} = x: x; \"${key}\" = 1; }"] {
        let result = extract_from_source("default.nix", source, None, None);
        assert_eq!(result.nodes.len(), 1, "{source}");
    }
    let result = extract_from_source(
        "default.nix",
        "rec { services.web.enable = true; \"static-name\" = x: x; }",
        None,
        None,
    );
    for name in ["services.web.enable", "\"static-name\""] {
        assert!(
            result
                .nodes
                .iter()
                .any(|n| n.name == name && n.is_exported == Some(true)),
            "{name}"
        );
    }
}

#[test]
fn case_064_sqlite_reindex_is_stable() {
    let path = std::env::temp_dir().join(format!("rustcodegraph-064-{}", std::process::id()));
    let _cleanup = TempDir(path.clone());
    std::fs::create_dir_all(&path).unwrap();
    std::fs::write(path.join("default.nix"), "let f = x: x; in { run = f; }").unwrap();
    let mut graph = rustcodegraph::CodeGraph::init_sync(&path).unwrap();
    let mut first = Vec::new();
    let mut first_edges = Vec::new();
    for pass in 0..2 {
        let result = graph.index_all(rustcodegraph::IndexOptions::default());
        assert!(result.success, "{:?}", result.errors);
        let mut nodes = graph
            .get_nodes_in_file("default.nix")
            .into_iter()
            .map(|n| (n.id, n.name, n.kind))
            .collect::<Vec<_>>();
        nodes.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(nodes.len(), 3, "{nodes:?}");
        let mut edges = nodes
            .iter()
            .flat_map(|(id, _, _)| graph.get_outgoing_edges(id))
            .filter(|e| e.kind == EdgeKind::Contains)
            .map(|e| (e.source, e.target))
            .collect::<Vec<_>>();
        edges.sort();
        assert_eq!(edges.len(), 2);
        if pass == 0 {
            first_edges = edges;
        } else {
            assert_eq!(edges, first_edges);
        }
        if pass == 0 {
            first = nodes;
        } else {
            assert_eq!(nodes, first);
        }
    }
    graph.close();
}

struct TempDir(std::path::PathBuf);
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
