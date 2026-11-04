use rustcodegraph::extraction::grammars::{
    detect_language, get_parser, is_language_supported, language_key,
};
use rustcodegraph::extraction::languages::index::extractor_for;
use rustcodegraph::extraction::tree_sitter::TreeSitterExtractor;

#[test]
fn case_171_detection_and_native_grammar() {
    let language = detect_language("Sample.cfm", None);
    assert_eq!(language_key(&language), "cfml");
    assert!(is_language_supported(language));
    assert_eq!(serde_json::to_string(&language).unwrap(), "\"cfml\"");
    assert_eq!(
        serde_json::from_str::<rustcodegraph::types::Language>("\"cfml\"").unwrap(),
        language
    );
    let mut parser = get_parser(language).expect("native CFML parser");
    let tree = parser
        .parse(
            "<cfcomponent><cffunction name=\"greet\"><cfreturn 1></cffunction></cfcomponent>",
            None,
        )
        .unwrap();
    assert!(!tree.root_node.has_error);
}

#[test]
fn case_171_empty_unicode_and_damaged() {
    let language = detect_language("sample.cfm", None);
    assert_eq!(language_key(&language), "cfml");
    let extractor = extractor_for("cfml").expect("registered CFML extractor");
    for (source, damaged) in [
        ("", false),
        ("<cfoutput>你好🌍</cfoutput>", false),
        ("<cfcomponent><cffunction name=", true),
    ] {
        let normalized = extractor
            .pre_parse(source)
            .unwrap_or_else(|| source.to_owned());
        let tree = get_parser(language)
            .unwrap()
            .parse(&normalized, None)
            .unwrap();
        assert_eq!(tree.root_node.has_error, damaged, "{normalized}");
        let result = TreeSitterExtractor::new("sample.cfm", source, None).extract();
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
fn case_171_extensions_reach_sqlite() {
    let dir = std::env::temp_dir().join(format!("rcg-171-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for file in ["sample.cfm", "sample.cfc", "sample.cfml", "UPPER.CFM"] {
        assert_eq!(language_key(&detect_language(file, None)), "cfml");
        std::fs::write(dir.join(file), "").unwrap();
    }
    assert_ne!(
        language_key(&detect_language("sample.cfm.txt", None)),
        "cfml"
    );
    let mut graph = rustcodegraph::CodeGraph::init_sync(&dir).unwrap();
    let result = graph.index_all(Default::default());
    assert!(result.success, "{:?}", result.errors);
    for file in ["sample.cfm", "sample.cfc", "sample.cfml", "UPPER.CFM"] {
        let nodes = graph.get_nodes_in_file(file);
        assert_eq!(nodes.len(), 1, "{file}");
        assert_eq!(language_key(&nodes[0].language), "cfml");
    }
    graph.close();
    std::fs::remove_dir_all(dir).unwrap();
}
