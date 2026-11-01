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
    compare(30, 0, false);
    compare(1001, 4, false);
}
#[test]
fn case_018_existing_transaction_falls_back() {
    compare(30, 4, true);
}

/// Opt-in real-source experiment: never indexes or writes into the source checkout.
#[test]
fn case_018_real_source_equivalence() {
    let Ok(root) = std::env::var("RCG18_CORPUS") else {
        return;
    };
    let root = std::path::PathBuf::from(root);
    let manifest = std::env::var("RCG18_MANIFEST").expect("source manifest");
    let workers: usize = std::env::var("RCG18_WORKERS")
        .unwrap_or("4".into())
        .parse()
        .unwrap();
    let dir = std::env::temp_dir().join(format!("rcg018-real-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("graph.db");
    let mut db = DatabaseConnection::initialize(&path).unwrap();
    let mut queries = QueryBuilder::new(db.get_db());
    let mut refs = Vec::new();
    let mut files = 0;
    for relative in std::fs::read_to_string(manifest).unwrap().lines() {
        let source = std::fs::read_to_string(root.join(relative)).unwrap();
        let extraction = rustcodegraph::extraction::tree_sitter::extract_from_source(
            relative, &source, None, None,
        );
        queries
            .upsert_file(&FileRecord {
                path: relative.into(),
                content_hash: String::new(),
                language: extraction
                    .nodes
                    .first()
                    .map_or(Language::Unknown, |n| n.language),
                size: source.len() as u64,
                modified_at: 0,
                indexed_at: 0,
                node_count: extraction.nodes.len() as u64,
                errors: None,
            })
            .unwrap();
        queries.insert_nodes(&extraction.nodes).unwrap();
        queries.insert_edges(&extraction.edges).unwrap();
        refs.extend(extraction.unresolved_references);
        files += 1;
    }
    assert!(files > 10 && refs.len() > 100);
    let mut resolver = ReferenceResolver::new(root.to_string_lossy(), queries);
    let started = std::time::Instant::now();
    let mut resolved = 0;
    for batch in refs.chunks(1000) {
        let result = resolver.resolve_all_readonly_parallel(batch, workers);
        resolved += result.stats.resolved;
        let edges = resolver.create_edges(&result.resolved);
        resolver.queries.insert_edges(&edges).unwrap();
    }
    let chained = resolver.resolve_chained_calls_via_conformance();
    let inherited = resolver.resolve_deferred_this_member_refs();
    let elapsed = started.elapsed();
    drop(resolver);
    db.close().unwrap();
    let connection = rusqlite::Connection::open(&path).unwrap();
    let mut statement = connection.prepare("SELECT source,target,kind,metadata,line,col,provenance FROM edges ORDER BY source,target,kind,metadata,line,col,provenance").unwrap();
    let mut rows = statement
        .query_map([], |row| {
            (0..7)
                .map(|i| row.get::<_, rusqlite::types::Value>(i))
                .collect::<Result<Vec<_>, _>>()
        })
        .unwrap()
        .map(|row| {
            let mut row = row.unwrap();
            if let rusqlite::types::Value::Text(text) = &row[3] {
                row[3] = rusqlite::types::Value::Text(
                    serde_json::to_string(
                        &serde_json::from_str::<serde_json::Value>(text).unwrap(),
                    )
                    .unwrap(),
                );
            }
            format!("{row:?}")
        })
        .collect::<Vec<_>>();
    rows.sort();
    use sha2::{Digest, Sha256};
    let digest = format!("{:x}", Sha256::digest(format!("{rows:?}").as_bytes()));
    eprintln!(
        "RCG18 files={files} refs={} resolved={resolved} chained={chained} inherited={inherited} workers={workers} elapsed_ms={} edges={} digest={digest}",
        refs.len(),
        elapsed.as_millis(),
        rows.len()
    );
    assert!(resolved > 0);
    if let Ok(expected) = std::env::var("RCG18_EXPECTED") {
        assert_eq!(digest, expected);
    }
    drop(statement);
    drop(connection);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn case_018_worker_connection_rejects_writes() {
    let dir = std::env::temp_dir().join(format!("rcg018-readonly-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("graph.db");
    let mut db = DatabaseConnection::initialize(&path).unwrap();
    db.close().unwrap();
    let mut reader = rustcodegraph::db::sqlite_adapter::create_readonly_database(&path).unwrap();
    assert!(reader.exec("CREATE TABLE forbidden (id INTEGER)").is_err());
    reader.close().unwrap();
    std::fs::remove_dir_all(dir).unwrap();
}
