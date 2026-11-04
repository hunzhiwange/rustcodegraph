use rustcodegraph::extraction::languages::index::extractor_for;
use rustcodegraph::extraction::{grammars::*, index::extract_from_source};

#[test]
fn case_099_detection_and_serialization() {
    let lang = detect_language("main.SOL", None);
    assert_eq!(serde_json::to_string(&lang).unwrap(), "\"solidity\"");
    assert_eq!(
        serde_json::from_str::<rustcodegraph::types::Language>("\"solidity\"").unwrap(),
        lang
    );
    for name in ["main.sol", "MAIN.SOL"] {
        assert!(is_source_file(name));
        assert_eq!(detect_language(name, None), lang);
    }
    assert!(!is_source_file("main.sol.bak"));
    assert!(extractor_for("solidity").is_some());
    assert!(is_language_supported(lang));
    assert!(get_supported_languages().contains(&lang));
    assert_eq!(get_language_display_name(lang), "Solidity");
    assert!(rustcodegraph::web_tree_sitter::Language::load("solidity").is_ok());
}

#[test]
fn case_099_native_parser() {
    let lang = detect_language("main.sol", None);
    let mut parser = get_parser(lang).expect("Solidity native parser");
    let tree = parser.parse("contract Demo {}", None).unwrap();
    assert_eq!(tree.root_node.type_name(), "source_file");
    assert!(!tree.root_node.has_error);
    assert!(format!("{:?}", tree.root_node).contains("contract_declaration"));
}

#[test]
fn case_099_boundary_inputs() {
    for source in [
        "",
        "contract Demo { string label = unicode\"你好🌍\"; }",
        "contract Broken { function (",
    ] {
        let mut parser = get_parser(detect_language("main.sol", None)).unwrap();
        let tree = parser.parse(source, None).unwrap();
        assert_eq!(tree.root_node.end_index(), source.len());
        assert_eq!(
            tree.root_node.has_error,
            source == "contract Broken { function ("
        );
        let result = extract_from_source("main.sol", source, None, None);
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
            "\"solidity\""
        );
    }
}

#[test]
fn case_099_real_files_and_sqlite() {
    let temp =
        TempDir(std::env::temp_dir().join(format!("rustcodegraph-099-{}", std::process::id())));
    std::fs::create_dir_all(temp.path()).unwrap();
    for (name, source) in [
        (
            "main.sol",
            "contract Demo { string label = unicode\"你好\"; }",
        ),
        ("empty.sol", ""),
        ("broken.sol", "contract Broken { function ("),
    ] {
        std::fs::write(temp.path().join(name), source).unwrap();
    }
    let mut graph = rustcodegraph::CodeGraph::init_sync(temp.path()).unwrap();
    let result = graph.index_all(rustcodegraph::IndexOptions::default());
    assert!(result.success, "{:?}", result.errors);
    assert_eq!(result.files_indexed, 3);
    for name in ["main.sol", "empty.sol", "broken.sol"] {
        let nodes = graph.get_nodes_in_file(name);
        assert_eq!(
            nodes
                .iter()
                .filter(|node| node.kind == rustcodegraph::types::NodeKind::File)
                .count(),
            1
        );
        assert_eq!(language_key(&nodes[0].language), "solidity");
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
