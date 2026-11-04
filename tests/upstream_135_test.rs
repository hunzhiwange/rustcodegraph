use rustcodegraph::extraction::grammars::{
    detect_language, get_parser, is_language_supported, language_key,
};
use rustcodegraph::extraction::languages::index::extractor_for;
use rustcodegraph::extraction::tree_sitter::TreeSitterExtractor;

#[test]
fn case_135_detection_and_native_grammar() {
    let language = detect_language("Sample.vb", None);
    assert_eq!(language_key(&language), "vbnet");
    assert!(is_language_supported(language));
    assert_eq!(serde_json::to_string(&language).unwrap(), "\"vbnet\"");
    assert_eq!(
        serde_json::from_str::<rustcodegraph::types::Language>("\"vbnet\"").unwrap(),
        language
    );
    let mut parser = get_parser(language).expect("native VB.NET parser");
    let tree = parser.parse("Class Sample\nEnd Class\n", None).unwrap();
    assert!(!tree.root_node.has_error);
}

#[test]
fn case_135_empty_unicode_and_damaged() {
    let language = detect_language("Sample.vb", None);
    assert_eq!(language_key(&language), "vbnet");
    let extractor = extractor_for("vbnet").expect("VB.NET extractor registered");
    for source in [
        "",
        "Class 示例\nSub 运行()\nDim text = \"你好🌍\"\nEnd Sub\nEnd Class",
        "Class Broken\nSub (\n",
    ] {
        let normalized = extractor
            .pre_parse(source)
            .unwrap_or_else(|| source.to_owned());
        assert!(normalized.ends_with('\n'));
        assert!(normalized.starts_with(source));
        let mut parser = get_parser(language).unwrap();
        let tree = parser.parse(&normalized, None).unwrap();
        if !source.contains("Broken") {
            assert!(!tree.root_node.has_error);
        } else {
            assert!(tree.root_node.has_error);
        }
        let result = TreeSitterExtractor::new("Sample.vb", source, None).extract();
        assert!(result.errors.is_empty(), "{:?}", result.errors);
    }
    assert_ne!(language_key(&detect_language("Sample.vbs", None)), "vbnet");
}

#[test]
fn case_135_real_file_reaches_sqlite() {
    let dir = std::env::temp_dir().join(format!("rcg-135-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("Sample.vb"), "Class 示例\nEnd Class").unwrap();
    let mut graph = rustcodegraph::CodeGraph::init_sync(&dir).unwrap();
    let result = graph.index_all(Default::default());
    assert!(result.success, "{:?}", result.errors);
    let nodes = graph.get_nodes_in_file("Sample.vb");
    assert_eq!(
        nodes.len(),
        1,
        "this task only wires the file-level extractor"
    );
    assert_eq!(language_key(&nodes[0].language), "vbnet");
    graph.close();
    std::fs::remove_dir_all(dir).unwrap();
}
