use rustcodegraph::extraction::languages::index::extractor_for;
use rustcodegraph::extraction::{grammars::*, index::extract_from_source};

#[test]
fn case_063_detection_and_serialization() {
    let lang = detect_language("flake.NIX", None);
    assert_eq!(serde_json::to_string(&lang).unwrap(), "\"nix\"");
    assert_eq!(
        serde_json::from_str::<rustcodegraph::types::Language>("\"nix\"").unwrap(),
        lang
    );
    assert!(is_source_file("default.nix"));
    assert!(!is_source_file("default.nix.bak"));
    assert!(extractor_for("nix").is_some());
    assert!(is_language_supported(lang));
    assert!(get_supported_languages().contains(&lang));
    assert_eq!(get_language_display_name(lang), "Nix");
    assert!(rustcodegraph::web_tree_sitter::Language::load("nix").is_ok());
}

#[test]
fn case_063_native_parser() {
    let lang = detect_language("default.nix", None);
    let mut parser = get_parser(lang).expect("Nix native parser");
    let tree = parser.parse("let f = x: x; in f 1", None).unwrap();
    assert_eq!(tree.root_node.type_name(), "source_code");
    assert!(!tree.root_node.has_error);
    assert!(format!("{:?}", tree.root_node).contains("function_expression"));
}

#[test]
fn case_063_boundary_inputs() {
    for source in ["", "{ message = \"你好🌍\"; }", "let x = ; in"] {
        let mut parser = get_parser(detect_language("default.nix", None)).unwrap();
        let tree = parser.parse(source, None).unwrap();
        assert_eq!(tree.root_node.end_index(), source.len());
        assert_eq!(tree.root_node.has_error, source == "let x = ; in");
        let result = extract_from_source("default.nix", source, None, None);
        assert_eq!(
            result
                .nodes
                .iter()
                .filter(|node| node.kind == rustcodegraph::types::NodeKind::File)
                .count(),
            1,
            "file node: {source}"
        );
        assert_eq!(
            serde_json::to_string(&result.nodes[0].language).unwrap(),
            "\"nix\""
        );
    }
}

#[test]
fn case_063_real_files_and_sqlite() {
    let temp =
        TempDir(std::env::temp_dir().join(format!("rustcodegraph-063-{}", std::process::id())));
    std::fs::create_dir_all(temp.path()).unwrap();
    for (name, source) in [
        ("default.nix", "{ message = \"你好\"; }"),
        ("empty.nix", ""),
        ("broken.nix", "let x = ; in"),
    ] {
        std::fs::write(temp.path().join(name), source).unwrap();
    }
    let mut graph = rustcodegraph::CodeGraph::init_sync(temp.path()).unwrap();
    let result = graph.index_all(rustcodegraph::IndexOptions::default());
    assert!(result.success, "{:?}", result.errors);
    assert_eq!(result.files_indexed, 3);
    for name in ["default.nix", "empty.nix", "broken.nix"] {
        let nodes = graph.get_nodes_in_file(name);
        assert_eq!(
            nodes
                .iter()
                .filter(|node| node.kind == rustcodegraph::types::NodeKind::File)
                .count(),
            1
        );
        assert_eq!(language_key(&nodes[0].language), "nix");
    }
    graph.close();
}

struct TempDir(std::path::PathBuf);
impl TempDir {
    fn path(&self) -> &std::path::Path {
        &self.0
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
