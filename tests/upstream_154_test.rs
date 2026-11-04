use rustcodegraph::extraction::tree_sitter::TreeSitterExtractor;
use rustcodegraph::types::{EdgeKind, NodeKind};

const SOURCE: &str = "IDENTIFICATION DIVISION.\nPROGRAM-ID. DEMO.\nPROCEDURE DIVISION.\nFIRST SECTION.\nONE.\n    DISPLAY 'hello'.\nTWO.\n    CONTINUE.\nLAST SECTION.\nTHREE.\n    STOP RUN.\n";

#[test]
fn case_154_program_sections_and_paragraph_ranges() {
    let result = TreeSitterExtractor::new("demo.cbl", SOURCE, None).extract();
    assert!(result.errors.is_empty());
    for (name, kind, start, end, qualified, parent) in [
        ("DEMO", NodeKind::Module, 1, 11, "DEMO", "demo.cbl"),
        ("FIRST", NodeKind::Function, 4, 8, "DEMO::FIRST", "DEMO"),
        ("ONE", NodeKind::Function, 5, 6, "DEMO::FIRST::ONE", "FIRST"),
        ("TWO", NodeKind::Function, 7, 8, "DEMO::FIRST::TWO", "FIRST"),
        ("LAST", NodeKind::Function, 9, 11, "DEMO::LAST", "DEMO"),
        (
            "THREE",
            NodeKind::Function,
            10,
            11,
            "DEMO::LAST::THREE",
            "LAST",
        ),
    ] {
        let node = result.nodes.iter().find(|n| n.name == name).expect(name);
        assert_eq!(node.kind, kind);
        assert_eq!((node.start_line, node.end_line), (start, end), "{name}");
        assert_eq!(node.qualified_name, format!("demo.cbl::{qualified}"));
        assert_eq!(node.is_exported, Some(false));
        let owner = result.nodes.iter().find(|n| n.name == parent).unwrap();
        assert!(
            result.edges.iter().any(|e| e.kind == EdgeKind::Contains
                && e.source == owner.id
                && e.target == node.id)
        );
    }
    assert_eq!(result.nodes.len(), 7);
}

#[test]
fn case_154_copybook_and_empty_boundaries() {
    for source in ["", "       *> No declarations\n"] {
        assert_eq!(
            TreeSitterExtractor::new("empty.cpy", source, None)
                .extract()
                .nodes
                .len(),
            1
        );
    }
    let source = "       ALONE.\n           CONTINUE.\n       OTHER.\n           STOP RUN.\n";
    let result = TreeSitterExtractor::new("labels.cpy", source, None).extract();
    let alone = result
        .nodes
        .iter()
        .find(|n| n.name == "ALONE")
        .expect("copybook paragraph");
    assert_eq!((alone.start_line, alone.end_line), (1, 2));
    assert_eq!(alone.qualified_name, "labels.cpy::ALONE");
    let next = result.nodes.iter().find(|n| n.name == "OTHER").unwrap();
    assert_eq!((next.start_line, next.end_line), (3, 4));
}

#[test]
fn case_154_sqlite_reindex_does_not_multiply_declarations() {
    let dir = std::env::temp_dir().join(format!("rcg-154-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("demo.cob"), SOURCE).unwrap();
    let mut graph = rustcodegraph::CodeGraph::init_sync(&dir).unwrap();
    for _ in 0..2 {
        let result = graph.index_all(Default::default());
        assert!(result.success, "{:?}", result.errors);
        assert_eq!(graph.get_nodes_in_file("demo.cob").len(), 7);
    }
    graph.close();
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn case_154_program_scope_resets_and_data_is_out_of_scope() {
    let source = "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. 'FIRST-PROG'.\n       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n       01 WS-VALUE PIC 9.\n       PROCEDURE DIVISION.\n       START-HERE.\n           STOP RUN.\n       END PROGRAM FIRST-PROG.\n       IDENTIFICATION DIVISION.\n       PROGRAM-ID. SECOND-PROG.\n       PROCEDURE DIVISION.\n       START-HERE.\n           STOP RUN.\n       END PROGRAM SECOND-PROG.\n";
    let result = TreeSitterExtractor::new("two.cbl", source, None).extract();
    assert!(result.errors.is_empty());
    assert_eq!(
        result.nodes.len(),
        5,
        "no data/statement nodes: {:?}",
        result.nodes
    );
    for (program, line) in [("FIRST-PROG", 7), ("SECOND-PROG", 13)] {
        let paragraph = result
            .nodes
            .iter()
            .find(|n| n.name == "START-HERE" && n.start_line == line)
            .unwrap();
        assert_eq!(
            paragraph.qualified_name,
            format!("two.cbl::{program}::START-HERE")
        );
        assert_eq!(paragraph.end_line, line + 1);
    }
}
