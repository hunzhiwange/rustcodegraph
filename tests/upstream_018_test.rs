use rustcodegraph::resolution::types::now_ms;

use rustcodegraph::resolution::index::ReferenceResolver;
use rustcodegraph::types::*;
use rustcodegraph::{DatabaseConnection, QueryBuilder};

fn make_node_named(id: &str, name: &str) -> Node {
    Node {
        id: id.to_owned(),
        kind: NodeKind::Function,
        name: name.to_owned(),
        qualified_name: name.to_owned(),
        file_path: "a.ts".to_owned(),
        language: Language::TypeScript,
        start_line: 1,
        end_line: 1,
        start_column: 0,
        end_column: 0,
        docstring: None,
        signature: None,
        visibility: None,
        is_exported: None,
        is_async: None,
        is_static: None,
        is_abstract: None,
        decorators: None,
        type_parameters: None,
        return_type: None,
        updated_at: now_ms(),
    }
}

fn compare(count: usize, workers: usize, transaction: bool) {
    let dir = std::env::temp_dir().join(format!(
        "rcg018-{}-{}-{count}-{workers}-{transaction}",
        std::process::id(),
        now_ms()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let mut db = DatabaseConnection::initialize(dir.join("graph.db")).unwrap();
    let mut queries = QueryBuilder::new(db.get_db());
    queries
        .insert_node(&make_node_named("target", "target"))
        .unwrap();
    queries
        .insert_node(&make_node_named("caller", "caller"))
        .unwrap();
    let refs = (0..count)
        .map(|i| UnresolvedReference {
            from_node_id: "caller".into(),
            reference_name: if i % 3 == 0 { "missing" } else { "target" }.into(),
            reference_kind: ReferenceKind::Calls,
            line: i as u64 + 1,
            column: 0,
            file_path: Some("a.ts".into()),
            language: Some(Language::TypeScript),
            candidates: None,
        })
        .collect::<Vec<_>>();
    drop(queries);
    if transaction {
        db.get_db().exec("BEGIN").unwrap();
    }
    let mut resolver =
        ReferenceResolver::new(dir.to_string_lossy(), QueryBuilder::new(db.get_db()));
    let serial = resolver.resolve_all(&refs, None);
    let parallel = resolver.resolve_all_readonly_parallel(&refs, workers);
    assert_eq!(serial.stats.total, parallel.stats.total);
    assert_eq!(serial.stats.by_method, parallel.stats.by_method);
    assert_eq!(serial.stats.resolved, parallel.stats.resolved);
    assert_eq!(serial.stats.unresolved, parallel.stats.unresolved);
    assert_eq!(
        serde_json::to_value(&serial.resolved).unwrap(),
        serde_json::to_value(&parallel.resolved).unwrap()
    );
    assert_eq!(
        serde_json::to_value(resolver.create_edges(&serial.resolved)).unwrap(),
        serde_json::to_value(resolver.create_edges(&parallel.resolved)).unwrap()
    );
    if count > 0 {
        assert!(serial.stats.resolved > 0);
        assert!(serial.stats.unresolved > 0);
    }
    drop(resolver);
    if transaction {
        db.get_db().exec("ROLLBACK").unwrap();
    }
    db.close().unwrap();
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn case_018_parallel_matches_serial() {
    compare(30, 4, false);
}
#[test]
fn case_018_empty_and_single_worker() {
    compare(0, 4, false);
    compare(30, 1, false);
}
#[test]
fn case_018_existing_transaction_falls_back() {
    compare(30, 4, true);
}
