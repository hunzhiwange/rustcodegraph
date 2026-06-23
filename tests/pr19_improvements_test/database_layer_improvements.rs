mod database_layer_improvements {
    use super::*;

    #[test]
    fn should_support_batch_insert_of_unresolved_refs() {
        if sqlite_unavailable() {
            return;
        }

        let test_dir = create_temp_dir();
        let db_path = test_dir.path().join("rustcodegraph.db");
        let mut db = DatabaseConnection::initialize(&db_path).expect("database should initialize");

        {
            let mut queries = QueryBuilder::new(db.get_db());
            queries
                .insert_node(&test_node("func:test:1", "testFunc", 1))
                .expect("node should insert");

            queries
                .insert_unresolved_refs_batch(&[
                    unresolved_call("func:test:1", "helperA", 2),
                    unresolved_call("func:test:1", "helperB", 3),
                ])
                .expect("unresolved refs should batch insert");

            let refs = queries
                .get_unresolved_references()
                .expect("unresolved refs should load");
            assert_eq!(refs.len(), 2);

            let mut names = refs
                .iter()
                .map(|reference| reference.reference_name.clone())
                .collect::<Vec<_>>();
            names.sort();
            assert_eq!(names, vec!["helperA".to_owned(), "helperB".to_owned()]);

            assert_eq!(refs[0].file_path.as_deref(), Some("test.ts"));
            assert_eq!(refs[0].language, Some(Language::TypeScript));
        }

        db.close().expect("database should close");
    }

    #[test]
    fn should_support_get_all_nodes() {
        if sqlite_unavailable() {
            return;
        }

        let test_dir = create_temp_dir();
        let db_path = test_dir.path().join("rustcodegraph.db");
        let mut db = DatabaseConnection::initialize(&db_path).expect("database should initialize");

        {
            let mut queries = QueryBuilder::new(db.get_db());
            for i in 0..3 {
                queries
                    .insert_node(&test_node(
                        &format!("func:test:{i}"),
                        &format!("func{i}"),
                        i * 10 + 1,
                    ))
                    .expect("node should insert");
            }

            let all_nodes = queries.get_all_nodes().expect("all nodes should load");
            assert_eq!(all_nodes.len(), 3);

            let mut names = all_nodes
                .iter()
                .map(|node| node.name.clone())
                .collect::<Vec<_>>();
            names.sort();
            assert_eq!(
                names,
                vec!["func0".to_owned(), "func1".to_owned(), "func2".to_owned()]
            );
        }

        db.close().expect("database should close");
    }

    #[test]
    fn should_set_performance_pragmas_on_initialization() {
        if sqlite_unavailable() {
            return;
        }

        let test_dir = create_temp_dir();
        let db_path = test_dir.path().join("rustcodegraph.db");
        let mut db = DatabaseConnection::initialize(&db_path).expect("database should initialize");

        assert_eq!(pragma_i64(&mut db, "synchronous"), 1);
        assert_eq!(pragma_i64(&mut db, "cache_size"), -64000);
        assert_eq!(pragma_i64(&mut db, "temp_store"), 2);
        assert_eq!(pragma_i64(&mut db, "mmap_size"), 268435456);

        db.close().expect("database should close");
    }

    #[test]
    fn should_handle_empty_batch_insert_gracefully() {
        if sqlite_unavailable() {
            return;
        }

        let test_dir = create_temp_dir();
        let db_path = test_dir.path().join("rustcodegraph.db");
        let mut db = DatabaseConnection::initialize(&db_path).expect("database should initialize");

        {
            let mut queries = QueryBuilder::new(db.get_db());
            queries
                .insert_unresolved_refs_batch(&[])
                .expect("empty unresolved ref batch should not throw");
        }

        db.close().expect("database should close");
    }
}
