use rustcodegraph::extraction::grammars::{
    detect_language, get_parser, is_language_supported, language_key,
};
use rustcodegraph::extraction::languages::index::extractor_for;
use rustcodegraph::extraction::tree_sitter::TreeSitterExtractor;

#[test]
fn case_153_detection_and_native_grammar() {
    let language = detect_language("Sample.cbl", None);
    assert_eq!(language_key(&language), "cobol");
    assert!(is_language_supported(language));
    assert_eq!(serde_json::to_string(&language).unwrap(), "\"cobol\"");
    assert_eq!(
        serde_json::from_str::<rustcodegraph::types::Language>("\"cobol\"").unwrap(),
        language
    );
    let mut parser = get_parser(language).expect("native COBOL parser");
    let tree = parser.parse("       IDENTIFICATION DIVISION.\n       PROGRAM-ID. SAMPLE.\n       PROCEDURE DIVISION.\n           STOP RUN.\n", None).unwrap();
    assert!(!tree.root_node.has_error);
}

#[test]
fn case_153_empty_unicode_and_damaged() {
    let language = detect_language("sample.cbl", None);
    assert_eq!(language_key(&language), "cobol");
    let extractor = extractor_for("cobol").expect("registered COBOL extractor");
    for (source, damaged) in [
        ("", false),
        ("    .", false),
        (
            "IDENTIFICATION DIVISION.\nPROGRAM-ID. DEMO.\nPROCEDURE DIVISION.\n    DISPLAY '你好🌍'.\n    STOP RUN.\n",
            false,
        ),
        (
            "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. DEMO.\n       PROCEDURE DIVISION.\n           CALL 'unterminated\n",
            true,
        ),
    ] {
        let normalized = extractor
            .pre_parse(source)
            .unwrap_or_else(|| source.to_owned());
        let tree = get_parser(language)
            .unwrap()
            .parse(&normalized, None)
            .unwrap();
        assert_eq!(tree.root_node.has_error, damaged, "{normalized}");
        let result = TreeSitterExtractor::new("sample.cbl", source, None).extract();
        assert!(result.errors.is_empty(), "{:?}", result.errors);
        assert_eq!(result.nodes.len(), 1, "only file scaffolding at this stage");
    }
}

#[test]
fn case_153_extensions_reach_sqlite() {
    let dir = std::env::temp_dir().join(format!("rcg-153-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for file in ["sample.cbl", "sample.cob", "sample.cpy", "UPPER.CBL"] {
        assert_eq!(language_key(&detect_language(file, None)), "cobol");
        std::fs::write(dir.join(file), "").unwrap();
    }
    assert_ne!(
        language_key(&detect_language("sample.cbl.txt", None)),
        "cobol"
    );
    let mut graph = rustcodegraph::CodeGraph::init_sync(&dir).unwrap();
    let result = graph.index_all(Default::default());
    assert!(result.success, "{:?}", result.errors);
    for file in ["sample.cbl", "sample.cob", "sample.cpy", "UPPER.CBL"] {
        let nodes = graph.get_nodes_in_file(file);
        assert_eq!(nodes.len(), 1, "{file}");
        assert_eq!(language_key(&nodes[0].language), "cobol");
    }
    graph.close();
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn case_153_free_format_margin_and_fixed_crlf() {
    let extractor = extractor_for("cobol").unwrap();
    let fixed = "000100 IDENTIFICATION DIVISION.\r\n000200 PROGRAM-ID. DEMO.\r\n000300 PROCEDURE DIVISION.\r\n000400     STOP RUN.\r\n";
    assert!(extractor.pre_parse(fixed).is_none());
    let free = format!(
        "identification division.\r\nprogram-id. DEMO.\r\nprocedure division.\r\n    display '{}'.\r\n    stop run.\r\n",
        "x".repeat(100)
    );
    let normalized = extractor.pre_parse(&free).unwrap();
    assert!(normalized.starts_with("CGWIDE "));
    assert_eq!(normalized.lines().count(), free.lines().count());
    for source in [fixed, normalized.as_str()] {
        let tree = get_parser(detect_language("test.cob", None))
            .unwrap()
            .parse(source, None)
            .unwrap();
        assert!(!tree.root_node.has_error, "{source}");
    }
}
