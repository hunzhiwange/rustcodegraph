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

#[test]
fn case_013_failed_batches_do_not_hide_later_references() {
    for (count, size) in [(0, 2), (9, 2), (9, 0)] {
        let dir =
            std::env::temp_dir().join(format!("rcg013-{}-{}-{size}", std::process::id(), now_ms()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut db = DatabaseConnection::initialize(dir.join("graph.db")).unwrap();
        let mut queries = QueryBuilder::new(db.get_db());
        for i in 0..count {
            queries
                .insert_node(&make_node_named(&format!("caller-{i}"), "caller"))
                .unwrap();
            queries
                .insert_unresolved_ref(&UnresolvedReference {
                    from_node_id: format!("caller-{i}"),
                    reference_name: if i % 2 == 0 {
                        format!("missing-{i}")
                    } else {
                        "caller".into()
                    },
                    reference_kind: ReferenceKind::Calls,
                    line: 1,
                    column: 0,
                    file_path: Some("main.rs".into()),
                    language: Some(Language::Rust),
                    candidates: None,
                })
                .unwrap();
        }
        let mut resolver = ReferenceResolver::new(dir.to_string_lossy(), queries);
        let result = resolver.resolve_and_persist_batched(None, size);
        assert_eq!(result.stats.total, if size == 0 { 0 } else { count });
        drop(resolver);
        db.close().unwrap();
        std::fs::remove_dir_all(dir).unwrap();
    }
}
