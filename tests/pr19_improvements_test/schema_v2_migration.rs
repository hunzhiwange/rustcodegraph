mod schema_v2_migration {
    use super::*;

    #[test]
    fn should_have_correct_current_schema_version() {
        if sqlite_unavailable() {
            return;
        }

        assert_eq!(CURRENT_SCHEMA_VERSION, 5);
    }

    #[test]
    fn should_have_migration_for_version_2() {
        if sqlite_unavailable() {
            return;
        }

        let test_dir = create_temp_dir();
        let db_path = test_dir.path().join("rustcodegraph.db");
        let mut db = DatabaseConnection::initialize(&db_path).expect("database should initialize");
        let pending = get_pending_migrations(db.get_db()).expect("pending migrations should load");

        assert!(migrations().iter().any(|migration| migration.version == 2));
        assert!(pending.is_empty());
        db.close().expect("database should close");
    }
}
