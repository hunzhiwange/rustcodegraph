//! Static fallback from upstream 15483713 vbnet-member-bodies.test.ts.
//! Get/Set bodies call ordinary methods; upstream has no VB event-dispatch synthesizer.
fn check(imports: &str, receiver: &str, expected: &[(&str, &str)]) {
    let dir = std::env::temp_dir().join(format!(
        "rcg139-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    for (file, text) in [
        ("Helper.vb", "Namespace One\nModule Helper\nFunction Compute() As Integer\nSink()\nReturn 1\nEnd Function\nSub Store()\nSink()\nEnd Sub\nSub Sink()\nEnd Sub\nEnd Module\nEnd Namespace\n".to_owned()),
        ("Other.vb", "Namespace Two\nModule Helper\nFunction Compute() As Integer\nReturn 2\nEnd Function\nSub Store()\nEnd Sub\nEnd Module\nEnd Namespace\n".to_owned()),
        ("Box.vb", format!("{imports}\nClass Box\nProperty Value As Integer\nGet\nReturn {receiver}.Compute()\nEnd Get\nSet(value As Integer)\n{receiver}.Store()\nEnd Set\nEnd Property\nEnd Class\n")),
    ] { std::fs::write(dir.join(file), text).unwrap(); }
    let mut graph = rustcodegraph::CodeGraph::init_sync(&dir).unwrap();
    assert!(graph.index_all(Default::default()).success);
    graph.close();
    let db = rusqlite::Connection::open(rustcodegraph::get_database_path(&dir)).unwrap();
    let mut stmt = db.prepare("SELECT s.name,t.name FROM edges e JOIN nodes s ON s.id=e.source JOIN nodes t ON t.id=e.target WHERE e.kind='calls' AND s.file_path LIKE '%Box.vb' ORDER BY t.name").unwrap();
    let calls: Vec<(String, String)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(
        calls,
        expected
            .iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect::<Vec<_>>()
    );
    if !expected.is_empty() {
        // Both accessor routes reach the same cross-file endpoint through static edges.
        let paths: i64 = db.query_row("SELECT count(*) FROM edges a JOIN nodes p ON p.id=a.source JOIN edges b ON b.source=a.target JOIN nodes t ON t.id=b.target WHERE p.name='Value' AND a.kind='calls' AND b.kind='calls' AND t.name='Sink'", [], |r| r.get(0)).unwrap();
        assert_eq!(paths, 2);
    }
    let synthetic: i64 = db
        .query_row(
            "SELECT count(*) FROM edges WHERE provenance='heuristic'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        synthetic, 0,
        "ordinary calls must not become heuristic dispatch"
    );
    drop(stmt);
    drop(db);
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn case_139_get_set_cross_file_flow() {
    check(
        "Imports Pick = One.Helper",
        "pick",
        &[("Value", "Compute"), ("Value", "Store")],
    );
}
#[test]
fn case_139_unknown_receiver_does_not_guess() {
    check("Imports One", "External.Helper", &[]);
}
#[test]
fn case_139_ambiguous_imports_do_not_guess() {
    check("Imports One\nImports Two", "Helper", &[]);
}
