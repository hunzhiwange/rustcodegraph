use rustcodegraph::extraction::tree_sitter::TreeSitterExtractor;
use rustcodegraph::types::ReferenceKind;

#[test]
fn case_155_static_calls_and_copy_with_owners() {
    let source = "IDENTIFICATION DIVISION.\nPROGRAM-ID. DEMO.\nDATA DIVISION.\nWORKING-STORAGE SECTION.\nCOPY 'COMMON'.\nPROCEDURE DIVISION.\nMAIN SECTION.\n    CALL 'BOOT'.\nONE.\n    PERFORM TWO THRU LAST.\n    IF 1 = 1\n        CALL 'WORKER'\n    END-IF.\nTWO.\n    COPY LOCAL.\nLAST.\n    STOP RUN.\n";
    let result = TreeSitterExtractor::new("demo.cbl", source, None).extract();
    assert!(result.errors.is_empty());
    let refs: Vec<_> = result
        .unresolved_references
        .iter()
        .map(|r| {
            let owner = result
                .nodes
                .iter()
                .find(|n| n.id == r.from_node_id)
                .unwrap();
            (
                owner.name.as_str(),
                r.reference_name.as_str(),
                r.reference_kind,
                r.line,
            )
        })
        .collect();
    assert_eq!(
        refs,
        vec![
            ("DEMO", "COMMON", ReferenceKind::Imports, 5),
            ("MAIN", "BOOT", ReferenceKind::Calls, 8),
            ("ONE", "TWO", ReferenceKind::Calls, 10),
            ("ONE", "LAST", ReferenceKind::Calls, 10),
            ("ONE", "WORKER", ReferenceKind::Calls, 12),
            ("TWO", "LOCAL", ReferenceKind::Imports, 15),
        ]
    );
}

#[test]
fn case_155_dynamic_and_text_are_not_calls() {
    let source = "IDENTIFICATION DIVISION.\nPROGRAM-ID. DEMO.\nPROCEDURE DIVISION.\nONE.\n    *> CALL 'COMMENT'\n    DISPLAY \"CALL 'TEXT' PERFORM FAKE\".\n    CALL WS-PROGRAM.\n    PERFORM 2 TIMES\n        DISPLAY '你好' CALL 'REAL'\n    END-PERFORM.\n";
    let result = TreeSitterExtractor::new("demo.cbl", source, None).extract();
    assert_eq!(
        result.unresolved_references.len(),
        1,
        "{:?}",
        result.unresolved_references
    );
    let r = &result.unresolved_references[0];
    assert_eq!(r.reference_name, "REAL");
    assert_eq!(r.line, 9);
    // Native parser columns are UTF-8 bytes, including the seven-column free-format prefix.
    assert_eq!(r.column, (7 + "        DISPLAY '你好' CALL ".len()) as u64);
}

#[test]
fn case_155_sqlite_copy_import_is_stable() {
    let dir = std::env::temp_dir().join(format!("rcg-155-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("demo.cbl"), "       COPY COMMON.\n").unwrap();
    let mut graph = rustcodegraph::CodeGraph::init_sync(&dir).unwrap();
    for _ in 0..2 {
        let result = graph.index_all(Default::default());
        assert!(result.success, "{:?}", result.errors);
        assert!(
            graph
                .get_nodes_in_file("demo.cbl")
                .iter()
                .any(|n| n.name == "COMMON" && n.kind == rustcodegraph::types::NodeKind::Import)
        );
    }
    graph.close();
    std::fs::remove_dir_all(dir).unwrap();
}
