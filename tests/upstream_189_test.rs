use rustcodegraph::extraction::grammars::{
    detect_language, get_parser, is_language_supported, language_key,
};
use rustcodegraph::extraction::languages::index::extractor_for;
use rustcodegraph::extraction::tree_sitter::TreeSitterExtractor;

#[test]
fn case_189_detection_and_native_grammar() {
    let language = detect_language("Sample.ets", None);
    assert_eq!(language_key(&language), "arkts");
    assert!(is_language_supported(language));
    assert_eq!(serde_json::to_string(&language).unwrap(), "\"arkts\"");
    assert_eq!(
        serde_json::from_str::<rustcodegraph::types::Language>("\"arkts\"").unwrap(),
        language
    );
    let mut parser = get_parser(language).expect("native ArkTS parser");
    let tree = parser
        .parse("@Component struct Home { build() { Text('hello') } }", None)
        .unwrap();
    assert!(!tree.root_node.has_error);
}

#[test]
fn case_189_empty_unicode_and_damaged() {
    let language = detect_language("sample.ets", None);
    assert_eq!(language_key(&language), "arkts");
    let extractor = extractor_for("arkts").expect("registered ArkTS extractor");
    for (source, damaged) in [
        ("", false),
        (
            "@Component struct Home { build() { Text('你好🌍') } }",
            false,
        ),
        ("struct Broken { build( {", true),
    ] {
        let normalized = extractor
            .pre_parse(source)
            .unwrap_or_else(|| source.to_owned());
        let tree = get_parser(language)
            .unwrap()
            .parse(&normalized, None)
            .unwrap();
        assert_eq!(tree.root_node.has_error, damaged, "{normalized}");
        let result = TreeSitterExtractor::new("sample.ets", source, None).extract();
        assert!(result.errors.is_empty(), "{:?}", result.errors);
        assert!(
            result
                .nodes
                .iter()
                .any(|n| n.kind == rustcodegraph::types::NodeKind::File)
        );
    }
}

#[test]
fn case_189_extensions_reach_sqlite() {
    let dir = std::env::temp_dir().join(format!("rcg-189-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for file in ["sample.ets", "UPPER.ETS"] {
        assert_eq!(language_key(&detect_language(file, None)), "arkts");
        std::fs::write(dir.join(file), "").unwrap();
    }
    assert_ne!(
        language_key(&detect_language("sample.ets.txt", None)),
        "arkts"
    );
    let mut graph = rustcodegraph::CodeGraph::init_sync(&dir).unwrap();
    let result = graph.index_all(Default::default());
    assert!(result.success, "{:?}", result.errors);
    for file in ["sample.ets", "UPPER.ETS"] {
        let nodes = graph.get_nodes_in_file(file);
        assert_eq!(nodes.len(), 1, "{file}");
        assert_eq!(language_key(&nodes[0].language), "arkts");
    }
    graph.close();
    std::fs::remove_dir_all(dir).unwrap();
}
