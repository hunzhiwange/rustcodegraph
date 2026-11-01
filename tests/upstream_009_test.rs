use rustcodegraph::resolution::{
    index::ReferenceResolver,
    types::{UnresolvedRef, now_ms},
};
use rustcodegraph::types::{Language, ReferenceKind};
use rustcodegraph::{DatabaseConnection, QueryBuilder};
#[test]
fn case_009_nested_destructured_and_single_line_bindings() {
    let dir = std::env::temp_dir().join(format!("rcg009-{}-{}", std::process::id(), now_ms()));
    std::fs::create_dir(&dir).unwrap();
    let source = "function outer({client}) { { let client; client.send(); } client.send(); }";
    std::fs::write(dir.join("a.js"), source).unwrap();
    let mut db = DatabaseConnection::initialize(dir.join("graph.db")).unwrap();
    let mut r = ReferenceResolver::new(dir.to_string_lossy(), QueryBuilder::new(db.get_db()));
    let make = |column| UnresolvedRef {
        from_node_id: "outer".into(),
        reference_name: "client.send".into(),
        reference_kind: ReferenceKind::Calls,
        line: 1,
        column,
        file_path: "a.js".into(),
        language: Language::JavaScript,
        candidates: None,
    };
    for (at, _) in source.match_indices("client.send") {
        assert!(r.js_local_binding_blocks(&make(at as u64)));
    }
    assert_eq!(r.js_binding_analysis_count(), 1);
    r.clear_caches();
    std::fs::write(dir.join("a.js"), "client.send();").unwrap();
    assert!(!r.js_local_binding_blocks(&make(0)));
    assert_eq!(r.js_binding_analysis_count(), 2);
    drop(r);
    db.close().unwrap();
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn case_009_cached_and_uncached_resolution_edges() {
    use rustcodegraph::types::{Node, NodeKind};
    let dir =
        std::env::temp_dir().join(format!("rcg009-edges-{}-{}", std::process::id(), now_ms()));
    std::fs::create_dir(&dir).unwrap();
    let source = "function f(client) { client.send(); } client.send(); const g = client => client.send(); function h(){ {var client;} client.send(); }";
    std::fs::write(dir.join("a.ts"), source).unwrap();
    let mut db = DatabaseConnection::initialize(dir.join("graph.db")).unwrap();
    let node:Node=serde_json::from_value(serde_json::json!({"id":"send", "kind":"method", "name":"send", "qualifiedName":"Client.send", "filePath":"a.ts", "language":"typescript", "startLine":1,"endLine":1,"startColumn":0,"endColumn":0,"updatedAt":now_ms()})).unwrap();
    let mut r = ReferenceResolver::new(dir.to_string_lossy(), QueryBuilder::new(db.get_db()));
    r.queries.insert_node(&node).unwrap();
    assert_eq!(node.kind, NodeKind::Method);
    let refs = source
        .match_indices("client.send")
        .map(|(at, _)| UnresolvedRef {
            from_node_id: format!("call-{at}"),
            reference_name: "client.send".into(),
            reference_kind: ReferenceKind::Calls,
            line: 1,
            column: at as u64,
            file_path: "a.ts".into(),
            language: Language::TypeScript,
            candidates: None,
        })
        .collect::<Vec<_>>();
    let cached = refs
        .iter()
        .map(|r0| {
            r.resolve_one(r0)
                .map(|x| (x.original.from_node_id, x.target_node_id))
        })
        .collect::<Vec<_>>();
    assert_eq!(r.js_binding_analysis_count(), 1);
    assert!(cached[0].is_none());
    assert!(cached[1].is_some());
    assert!(cached[2].is_none());
    assert!(cached[3].is_none());
    let uncached = refs
        .iter()
        .map(|r0| {
            r.clear_caches();
            r.resolve_one(r0)
                .map(|x| (x.original.from_node_id, x.target_node_id))
        })
        .collect::<Vec<_>>();
    assert_eq!(cached, uncached);
    assert_eq!(r.js_binding_analysis_count(), 1 + refs.len());
    drop(r);
    db.close().unwrap();
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn case_009_known_callable_and_sibling_scope() {
    let dir =
        std::env::temp_dir().join(format!("rcg009-scope-{}-{}", std::process::id(), now_ms()));
    std::fs::create_dir(&dir).unwrap();
    let source = "{let client; client.send();} client.send(); const action=()=>{}; action();";
    std::fs::write(dir.join("a.js"), source).unwrap();
    let mut db = DatabaseConnection::initialize(dir.join("graph.db")).unwrap();
    let mut r = ReferenceResolver::new(dir.to_string_lossy(), QueryBuilder::new(db.get_db()));
    for (name, expected) in [
        ("client.send", vec![true, false]),
        ("action()", vec![false]),
    ] {
        for ((at, _), blocked) in source.match_indices(name).zip(expected) {
            let reference = UnresolvedRef {
                from_node_id: "f".into(),
                reference_name: name.trim_end_matches("()").into(),
                reference_kind: ReferenceKind::Calls,
                line: 1,
                column: at as u64,
                file_path: "a.js".into(),
                language: Language::JavaScript,
                candidates: None,
            };
            assert_eq!(r.js_local_binding_blocks(&reference), blocked);
        }
    }
    drop(r);
    db.close().unwrap();
    std::fs::remove_dir_all(dir).unwrap();
}
