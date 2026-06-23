mod best_candidate_resolution {
    use super::*;

    #[test]
    fn should_be_testable_via_the_resolution_module_types() {
        if sqlite_unavailable() {
            return;
        }

        let test_dir = create_temp_dir();
        let db_path = test_dir.path().join("rustcodegraph.db");
        let mut db = DatabaseConnection::initialize(&db_path).expect("database should initialize");

        {
            let queries = QueryBuilder::new(db.get_db());
            let mut resolver =
                ReferenceResolver::new(test_dir.path().to_string_lossy().into_owned(), queries);
            let unresolved = UnresolvedRef {
                from_node_id: "func:test:1".to_owned(),
                reference_name: "helper".to_owned(),
                reference_kind: ReferenceKind::Calls,
                line: 1,
                column: 0,
                file_path: "test.ts".to_owned(),
                language: Language::TypeScript,
                candidates: None,
            };

            let _ = resolver.resolve_one(&unresolved);
        }

        db.close().expect("database should close");
    }
}
