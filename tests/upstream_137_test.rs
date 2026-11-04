use rustcodegraph::extraction::tree_sitter::TreeSitterExtractor;
use rustcodegraph::types::ReferenceKind;

#[test]
fn case_137_import_inheritance_and_accessor_calls() {
    let source = "Imports AliasName = System.Text\nClass Widget\nInherits BaseWidget\nImplements IWidget\nPublic Property Value As String\nGet\nReturn ReadValue()\nEnd Get\nSet(value As String)\nWriteValue(value)\nEnd Set\nEnd Property\nEnd Class\n";
    let result = TreeSitterExtractor::new("Sample.vb", source, None).extract();
    for (name, kind, owner) in [
        ("System.Text", ReferenceKind::Imports, None),
        ("BaseWidget", ReferenceKind::Extends, Some("Widget")),
        ("IWidget", ReferenceKind::Implements, Some("Widget")),
        ("ReadValue", ReferenceKind::Calls, Some("Value")),
        ("WriteValue", ReferenceKind::Calls, Some("Value")),
    ] {
        let r = result
            .unresolved_references
            .iter()
            .find(|r| r.reference_name == name && r.reference_kind == kind)
            .expect(name);
        if let Some(owner) = owner {
            assert_eq!(
                r.from_node_id,
                result.nodes.iter().find(|n| n.name == owner).unwrap().id
            );
        }
    }
}

#[test]
fn case_137_event_handler_nested_calls_and_positions() {
    let source = "Class 示例\nEvent Changed()\nSub OnChanged() Handles Me.Changed\n' Ghost()\nDim text = \"Fake()\"\nConsole.WriteLine(Wrap(Of String)(text))\nEnd Sub\nClass Inner\nSub Run()\nWork()\nEnd Sub\nEnd Class\nEnd Class";
    let result = TreeSitterExtractor::new("Unicode.vb", source, None).extract();
    for (name, owner, line) in [
        ("Console.WriteLine", "OnChanged", 6),
        ("Wrap", "OnChanged", 6),
        ("Work", "Run", 10),
    ] {
        let r = result
            .unresolved_references
            .iter()
            .find(|r| r.reference_name == name && r.reference_kind == ReferenceKind::Calls)
            .expect(name);
        assert_eq!(
            r.from_node_id,
            result.nodes.iter().find(|n| n.name == owner).unwrap().id
        );
        assert_eq!(r.line, line);
    }
    assert!(
        !result
            .unresolved_references
            .iter()
            .any(|r| r.reference_name.contains("Ghost") || r.reference_name.contains("Fake"))
    );
    assert!(
        result
            .unresolved_references
            .iter()
            .any(|r| r.reference_name == "Me.Changed"
                && r.reference_kind == ReferenceKind::References)
    );
}

#[test]
fn case_137_comments_strings_and_empty_file() {
    for source in [
        "",
        "' Imports Fake\n' Inherits Ghost\n' Call Fake()\n",
        "Class Safe\nProperty Text As String = \"Fake()\"\nEnd Class\n",
    ] {
        let result = TreeSitterExtractor::new("Empty.vb", source, None).extract();
        assert!(
            result.unresolved_references.is_empty(),
            "{:?}",
            result.unresolved_references
        );
    }
}

#[test]
fn case_137_multibyte_argument_column_and_no_final_edges() {
    let line = "Console.WriteLine(\"你好🌍\", Work())";
    let source = format!("Class Sample\nSub Run()\n{line}\nEnd Sub\nEnd Class\n");
    let result = TreeSitterExtractor::new("Columns.vb", &source, None).extract();
    let work = result
        .unresolved_references
        .iter()
        .find(|r| r.reference_name == "Work")
        .unwrap();
    assert_eq!(
        (work.line, work.column),
        (3, line.find("Work").unwrap() as u64)
    );
    assert_eq!(
        result
            .unresolved_references
            .iter()
            .filter(|r| r.reference_kind == ReferenceKind::Calls)
            .count(),
        2
    );
    assert!(
        result
            .edges
            .iter()
            .all(|e| e.kind == rustcodegraph::types::EdgeKind::Contains)
    );
}

#[test]
fn case_137_sqlite_indexes_imports_and_method_calls() {
    let dir = std::env::temp_dir().join(format!("rcg-137-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("Sample.vb"), "Imports External.Library\nClass Example\nInherits ExternalBase\nSub Run()\nMissingHelper()\nEnd Sub\nSub MissingHelper()\nEnd Sub\nEnd Class\n").unwrap();
    let mut graph = rustcodegraph::CodeGraph::init_sync(&dir).unwrap();
    assert!(graph.index_all(Default::default()).success);
    graph.close();
    let path = rustcodegraph::get_database_path(&dir);
    let db = rusqlite::Connection::open(path).unwrap();
    let imports: i64 = db
        .query_row(
            "SELECT count(*) FROM nodes WHERE name = 'External.Library' AND kind = 'import'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(imports, 1);
    let calls: i64 = db.query_row("SELECT count(*) FROM edges e JOIN nodes caller ON caller.id = e.source JOIN nodes callee ON callee.id = e.target WHERE e.kind = 'calls' AND caller.name = 'Run' AND callee.name = 'MissingHelper'", [], |r| r.get(0)).unwrap();
    assert_eq!(calls, 1);
    drop(db);
    std::fs::remove_dir_all(dir).unwrap();
}
